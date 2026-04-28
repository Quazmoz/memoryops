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
    slack::{parser::parse_slack_event, parser::ParsedSlackEvent, validator::verify_signature},
    store::find_raw_event_id_by_idempotency_key,
    STREAM_KEY,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlackIngestResponse {
    status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    event_id: Option<Uuid>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UrlVerificationResponse {
    challenge: String,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct SlackIntegration {
    workspace_id: Uuid,
    signing_secret: String,
}

#[axum::debug_handler]
#[tracing::instrument(skip(state, headers, body))]
pub async fn handle_slack_webhook(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, AppError> {
    let payload = serde_json::from_slice::<Value>(&body)
        .map_err(|error| AppError::Validation(format!("invalid JSON payload: {error}")))?;

    if let Some(challenge) = url_verification_challenge(&payload)? {
        return Ok((StatusCode::OK, Json(UrlVerificationResponse { challenge })).into_response());
    }

    let integration = matching_slack_integration(&state, &headers, &body).await?;
    let parsed = parse_slack_event(&payload)?;
    let idempotency_key = parsed.idempotency_key();

    if let Some(event_id) =
        find_raw_event_id_by_idempotency_key(&state.db, &idempotency_key).await?
    {
        return Ok((
            StatusCode::OK,
            Json(SlackIngestResponse {
                status: "duplicate".to_owned(),
                event_id: Some(event_id),
            }),
        )
            .into_response());
    }

    let event =
        insert_and_publish_slack_event(&state, integration.workspace_id, &parsed, idempotency_key)
            .await?;
    INGEST_EVENTS.add(1, &[]);

    Ok((
        StatusCode::ACCEPTED,
        Json(SlackIngestResponse {
            status: "accepted".to_owned(),
            event_id: Some(event.id),
        }),
    )
        .into_response())
}

fn url_verification_challenge(payload: &Value) -> Result<Option<String>, AppError> {
    if payload.get("type").and_then(Value::as_str) != Some("url_verification") {
        return Ok(None);
    }

    let challenge = payload
        .get("challenge")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .ok_or_else(|| AppError::Validation("missing Slack challenge".to_owned()))?;

    Ok(Some(challenge))
}

async fn matching_slack_integration(
    state: &AppState,
    headers: &HeaderMap,
    body: &[u8],
) -> Result<SlackIntegration, AppError> {
    let integrations = sqlx::query_as::<_, SlackIntegration>(
        r#"
        SELECT workspace_id,
            COALESCE(webhook_secret, webhook_secret_hash) AS signing_secret
        FROM integrations
        WHERE source = 'slack'
          AND deleted_at IS NULL
        ORDER BY workspace_id ASC
        "#,
    )
    .fetch_all(&state.db)
    .await
    .map_err(AppError::Database)?;

    for integration in integrations {
        if verify_signature(headers, body, &integration.signing_secret).is_ok() {
            return Ok(integration);
        }
    }

    Err(AppError::Unauthorized)
}

async fn insert_and_publish_slack_event(
    state: &AppState,
    workspace_id: Uuid,
    parsed: &ParsedSlackEvent,
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
            occurred_at,
            slack_channel,
            slack_thread_ts
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
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
    .bind(Source::Slack)
    .bind(parsed.event_type)
    .bind(&parsed.actor)
    .bind(&parsed.payload)
    .bind(&idempotency_key)
    .bind(parsed.occurred_at)
    .bind(&parsed.channel_id)
    .bind(&parsed.thread_ts)
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

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn url_verification_challenge_is_returned() {
        let payload = json!({
            "type": "url_verification",
            "challenge": "challenge-token"
        });

        let challenge = match url_verification_challenge(&payload) {
            Ok(Some(challenge)) => challenge,
            Ok(None) => panic!("url_verification should return a challenge"),
            Err(error) => panic!("url_verification should parse: {error}"),
        };

        assert_eq!(challenge, "challenge-token");
    }
}
