use anyhow::anyhow;
use axum::{
    body::Bytes,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use common::{
    models::{EventType, RawEvent, Source},
    telemetry::INGEST_EVENTS,
    AppError, AppState,
};
use redis::aio::ConnectionManager;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use crate::{
    jira::{parser::parse_jira_event, parser::ParsedJiraEvent, validator::verify_signature},
    store::find_raw_event_id_by_idempotency_key,
    STREAM_KEY,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JiraIngestResponse {
    status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    event_id: Option<Uuid>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct JiraIntegration {
    workspace_id: Uuid,
    signing_secret: Option<String>,
}

#[axum::debug_handler]
#[tracing::instrument(skip(state, headers, body))]
pub async fn handle_jira_webhook(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, AppError> {
    let payload = serde_json::from_slice::<Value>(&body)
        .map_err(|error| AppError::Validation(format!("invalid JSON payload: {error}")))?;
    let integration = matching_jira_integration(&state, &headers, &body).await?;
    let parsed = parse_jira_event(&payload)?;
    let idempotency_key = parsed.idempotency_key();

    if let Some(event_id) =
        find_raw_event_id_by_idempotency_key(&state.db, &idempotency_key).await?
    {
        return Ok((
            StatusCode::OK,
            Json(JiraIngestResponse {
                status: "duplicate".to_owned(),
                event_id: Some(event_id),
            }),
        )
            .into_response());
    }

    let event =
        insert_and_publish_jira_event(&state, integration.workspace_id, &parsed, idempotency_key)
            .await?;
    INGEST_EVENTS.add(1, &[]);

    Ok((
        StatusCode::ACCEPTED,
        Json(JiraIngestResponse {
            status: "accepted".to_owned(),
            event_id: Some(event.id),
        }),
    )
        .into_response())
}

async fn matching_jira_integration(
    state: &AppState,
    headers: &HeaderMap,
    body: &[u8],
) -> Result<JiraIntegration, AppError> {
    let integrations = sqlx::query_as::<_, JiraIntegration>(
        r#"
        SELECT workspace_id,
            NULLIF(webhook_secret, '') AS signing_secret
        FROM integrations
        WHERE source = 'jira'
          AND deleted_at IS NULL
        ORDER BY workspace_id ASC
        "#,
    )
    .fetch_all(&state.db)
    .await
    .map_err(AppError::Database)?;

    for integration in integrations {
        let Some(secret) = integration.signing_secret.as_deref().map(str::trim) else {
            continue;
        };
        if !secret.is_empty() && verify_signature(headers, body, secret).is_ok() {
            return Ok(integration);
        }
    }

    Err(AppError::Unauthorized)
}

async fn insert_and_publish_jira_event(
    state: &AppState,
    workspace_id: Uuid,
    parsed: &ParsedJiraEvent,
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
    .bind(Source::Jira)
    .bind(parsed.event_type)
    .bind(&parsed.actor)
    .bind(&parsed.payload)
    .bind(&idempotency_key)
    .bind(parsed.occurred_at)
    .fetch_one(&mut *transaction)
    .await
    .map_err(AppError::Database)?;

    let mut redis = state.redis.clone();
    publish_raw_event_strict(&mut redis, &event).await?;
    transaction.commit().await.map_err(AppError::Database)?;

    Ok(event)
}

async fn publish_raw_event_strict(
    redis: &mut ConnectionManager,
    event: &RawEvent,
) -> Result<(), AppError> {
    redis::cmd("XADD")
        .arg(STREAM_KEY)
        .arg("*")
        .arg("event_id")
        .arg(event.id.to_string())
        .arg("workspace_id")
        .arg(event.workspace_id.to_string())
        .arg("source")
        .arg(source_as_str(event.source))
        .arg("event_type")
        .arg(event_type_as_str(event.event_type))
        .query_async::<String>(&mut *redis)
        .await
        .map(|_| ())
        .map_err(|error| AppError::Internal(anyhow!(error)))
}

fn source_as_str(source: Source) -> &'static str {
    match source {
        Source::GitHub => "github",
        Source::Slack => "slack",
        Source::Jira => "jira",
        Source::Linear => "linear",
    }
}

fn event_type_as_str(event_type: EventType) -> &'static str {
    match event_type {
        EventType::PullRequest => "pull_request",
        EventType::PullRequestReview => "pull_request_review",
        EventType::Push => "push",
        EventType::IssueComment => "issue_comment",
        EventType::Issue => "issue",
        EventType::Message => "message",
        EventType::Reaction => "reaction",
    }
}
