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
    queue::{publish_raw_event_with_mode, PublishMode},
    slack::{parser::parse_slack_event, parser::ParsedSlackEvent, validator::verify_signature},
    store::{insert_raw_event_in_tx, raw_event_needs_publish, InsertRawEventOutcome, NewRawEvent},
    webhook::workspace_webhook_secret,
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

#[axum::debug_handler]
#[tracing::instrument(skip(state, headers, body))]
pub async fn handle_slack_webhook(
    State(state): State<AppState>,
    Path(workspace_id): Path<Uuid>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, AppError> {
    let payload = serde_json::from_slice::<Value>(&body)
        .map_err(|error| AppError::Validation(format!("invalid JSON payload: {error}")))?;

    verify_slack_integration(&state, workspace_id, &headers, &body).await?;

    if let Some(challenge) = url_verification_challenge(&payload)? {
        return Ok((StatusCode::OK, Json(UrlVerificationResponse { challenge })).into_response());
    }

    let parsed = parse_slack_event(&payload)?;
    let idempotency_key = parsed.idempotency_key();

    let (event, duplicate) =
        insert_and_publish_slack_event(&state, workspace_id, &parsed, idempotency_key).await?;
    if duplicate {
        return Ok((
            StatusCode::OK,
            Json(SlackIngestResponse {
                status: "duplicate".to_owned(),
                event_id: Some(event.id),
            }),
        )
            .into_response());
    }

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

async fn verify_slack_integration(
    state: &AppState,
    workspace_id: Uuid,
    headers: &HeaderMap,
    body: &[u8],
) -> Result<(), AppError> {
    let secret = workspace_webhook_secret(state, workspace_id, Source::Slack)
        .await?
        .ok_or(AppError::Unauthorized)?;
    verify_signature(headers, body, &secret)
}

async fn insert_and_publish_slack_event(
    state: &AppState,
    workspace_id: Uuid,
    parsed: &ParsedSlackEvent,
    idempotency_key: String,
) -> Result<(RawEvent, bool), AppError> {
    let mut transaction = state.db.begin().await.map_err(AppError::Database)?;
    let event = match insert_raw_event_in_tx(
        &mut transaction,
        &NewRawEvent {
            workspace_id,
            source: Source::Slack,
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
