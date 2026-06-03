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
    linear::{parser::parse_linear_event, parser::ParsedLinearEvent, validator::verify_signature},
    queue::{publish_raw_event_with_mode, PublishMode},
    store::{insert_raw_event_in_tx, raw_event_needs_publish, InsertRawEventOutcome, NewRawEvent},
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

    let (event, duplicate) =
        insert_and_publish_linear_event(&state, workspace_id, &parsed, idempotency_key).await?;
    if duplicate {
        return Ok((
            StatusCode::OK,
            Json(LinearIngestResponse {
                status: "duplicate".to_owned(),
                event_id: Some(event.id),
            }),
        )
            .into_response());
    }

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
    let secret = workspace_webhook_secret(state, workspace_id, Source::Linear)
        .await?
        .map(|secret| secret.trim().to_owned())
        .filter(|secret| !secret.is_empty())
        .ok_or(AppError::Unauthorized)?;

    verify_signature(headers, body, &secret)
}

async fn insert_and_publish_linear_event(
    state: &AppState,
    workspace_id: Uuid,
    parsed: &ParsedLinearEvent,
    idempotency_key: String,
) -> Result<(RawEvent, bool), AppError> {
    let mut transaction = state.db.begin().await.map_err(AppError::Database)?;
    let event = match insert_raw_event_in_tx(
        &mut transaction,
        &NewRawEvent {
            workspace_id,
            source: Source::Linear,
            event_type: parsed.event_type,
            actor: parsed.actor.clone(),
            payload: parsed.payload.clone(),
            idempotency_key,
            occurred_at: parsed.occurred_at,
        },
    )
    .await?
    {
        InsertRawEventOutcome::Existing(event) => {
            drop(transaction);
            if raw_event_needs_publish(&state.db, event.id).await? {
                publish_event(state, &event).await?;
            }
            return Ok((event, true));
        }
        InsertRawEventOutcome::Inserted(event) => event,
    };

    transaction.commit().await.map_err(AppError::Database)?;
    publish_event(state, &event).await?;

    Ok((event, false))
}

async fn publish_event(state: &AppState, event: &RawEvent) -> Result<(), AppError> {
    let mut redis = state
        .redis
        .get()
        .await
        .map_err(|error| AppError::Internal(anyhow!(error)))?;
    publish_raw_event_with_mode(&mut *redis, event, PublishMode::Strict).await
}
