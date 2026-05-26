use anyhow::anyhow;
use axum::{
    body::Bytes,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use common::{
    models::{RawEvent, Source},
    telemetry::INGEST_EVENTS,
    AppError, AppState,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use crate::{
    linear::{
        parser::parse_linear_event,
        parser::ParsedLinearEvent,
        validator::{self, verify_signature},
    },
    queue::{publish_raw_event_with_mode, PublishMode},
    store::find_raw_event_id_by_idempotency_key,
    webhook::workspace_webhook_secret,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LinearIngestResponse {
    status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    event_id: Option<Uuid>,
}

#[axum::debug_handler]
#[tracing::instrument(skip(state, headers, body))]
pub async fn handle_linear_webhook(
    State(state): State<AppState>,
    Path(workspace_id): Path<Uuid>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, AppError> {
    let payload = serde_json::from_slice::<Value>(&body)
        .map_err(|error| AppError::Validation(format!("invalid JSON payload: {error}")))?;
    verify_linear_integration(&state, workspace_id, &headers, &body).await?;
    let parsed = parse_linear_event(&payload)?;
    let idempotency_key = parsed.idempotency_key();

    if let Some(event_id) =
        find_raw_event_id_by_idempotency_key(&state.db, &idempotency_key).await?
    {
        return Ok((
            StatusCode::OK,
            Json(LinearIngestResponse {
                status: "duplicate".to_owned(),
                event_id: Some(event_id),
            }),
        )
            .into_response());
    }

    let event =
        insert_and_publish_linear_event(&state, workspace_id, &parsed, idempotency_key).await?;
    INGEST_EVENTS.add(1, &[]);

    Ok((
        StatusCode::ACCEPTED,
        Json(LinearIngestResponse {
            status: "accepted".to_owned(),
            event_id: Some(event.id),
        }),
    )
        .into_response())
}

async fn verify_linear_integration(
    state: &AppState,
    workspace_id: Uuid,
    headers: &HeaderMap,
    body: &[u8],
) -> Result<(), AppError> {
    let has_signature = headers.get(validator::SIGNATURE_HEADER).is_some();
    let secret = workspace_webhook_secret(state, workspace_id, Source::Linear).await?;

    if let Some(secret) = secret
        .as_deref()
        .map(str::trim)
        .filter(|secret| !secret.is_empty())
    {
        return verify_signature(headers, body, secret);
    }

    if !has_signature {
        tracing::warn!(
            workspace_id = %workspace_id,
            "accepting unsigned Linear webhook because no signing secret is configured"
        );
        return Ok(());
    }

    Err(AppError::Unauthorized)
}

async fn insert_and_publish_linear_event(
    state: &AppState,
    workspace_id: Uuid,
    parsed: &ParsedLinearEvent,
    idempotency_key: String,
) -> Result<RawEvent, AppError> {
    let mut transaction = state.db.begin().await.map_err(AppError::Database)?;
    let event = sqlx::query_as::<_, RawEvent>(
        r#"
        INSERT INTO raw_events (
            id,
            workspace_id,
            source,
            event_type,
            actor,
            payload,
            idempotency_key,
            occurred_at
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
        RETURNING id,
            workspace_id,
            source,
            event_type,
            actor,
            payload,
            idempotency_key,
            occurred_at,
            ingested_at
        "#,
    )
    .bind(Uuid::now_v7())
    .bind(workspace_id)
    .bind(Source::Linear)
    .bind(parsed.event_type)
    .bind(&parsed.actor)
    .bind(&parsed.payload)
    .bind(&idempotency_key)
    .bind(parsed.occurred_at)
    .fetch_one(&mut *transaction)
    .await
    .map_err(AppError::Database)?;

    let mut redis = state
        .redis
        .get()
        .await
        .map_err(|error| AppError::Internal(anyhow!(error)))?;
    publish_raw_event_with_mode(&mut *redis, &event, PublishMode::Strict).await?;
    transaction.commit().await.map_err(AppError::Database)?;

    Ok(event)
}
