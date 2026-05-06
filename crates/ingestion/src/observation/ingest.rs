use chrono::Utc;
use common::{
    audit::spawn_audit_log,
    error::AppResult,
    models::{AuditAction, EventType, Source},
    telemetry::INGEST_EVENTS,
    AppError, AppState,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::{
    queue::publish_raw_event,
    store::{find_raw_event_id_by_idempotency_key, insert_raw_event, NewRawEvent},
};

const MAX_CONTENT_LEN: usize = 8_000;
const MAX_TAGS: usize = 20;

#[derive(Debug, Clone, Deserialize)]
pub struct ObservationInput {
    pub content: String,
    pub agent_id: String,
    pub user_id: Option<String>,
    pub repo: Option<String>,
    pub tags: Option<Vec<String>>,
    pub importance: Option<f32>,
    pub source_ref: Option<String>,
    pub scope_id: Option<Uuid>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ObservationOutput {
    pub id: Uuid,
    pub status: &'static str,
}

pub async fn ingest_observation(
    state: &AppState,
    workspace_id: Uuid,
    input: ObservationInput,
) -> AppResult<ObservationOutput> {
    validate_input(&input)?;

    let idempotency_key = idempotency_key(workspace_id, &input.agent_id, &input.content);

    if find_raw_event_id_by_idempotency_key(&state.db, &idempotency_key)
        .await?
        .is_some()
    {
        let existing_id = idempotency_key_to_uuid(&idempotency_key);
        return Ok(ObservationOutput {
            id: existing_id,
            status: "queued",
        });
    }

    let payload = build_payload(&input);
    let actor = input.agent_id.clone();

    let event = insert_raw_event(
        &state.db,
        &NewRawEvent {
            workspace_id,
            source: Source::Observation,
            event_type: EventType::AgentObservation,
            actor: actor.clone(),
            payload,
            idempotency_key,
            occurred_at: Utc::now(),
        },
    )
    .await?;
    INGEST_EVENTS.add(1, &[]);

    let redis = state.redis.clone();
    let queued_event = event.clone();
    tokio::spawn(async move {
        let mut redis = match redis.get().await {
            Ok(redis) => redis,
            Err(error) => {
                tracing::error!(error = ?error, event_id = %queued_event.id, "failed to get Redis connection for raw event publish");
                return;
            }
        };
        let _ = publish_raw_event(&mut *redis, &queued_event).await;
    });

    spawn_audit_log(
        state.db.clone(),
        workspace_id,
        actor,
        AuditAction::ObservationIngested,
        event.id,
        "raw_event",
        None,
    );

    Ok(ObservationOutput {
        id: event.id,
        status: "queued",
    })
}

pub fn idempotency_key(workspace_id: Uuid, agent_id: &str, content: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(workspace_id.as_bytes());
    hasher.update(b":");
    hasher.update(agent_id.as_bytes());
    hasher.update(b":");
    hasher.update(content.as_bytes());
    let digest = hasher.finalize();
    format!("obs:{}", hex::encode(digest))
}

fn validate_input(input: &ObservationInput) -> AppResult<()> {
    let content_len = input.content.chars().count();
    if content_len == 0 {
        return Err(AppError::Validation("content is required".to_owned()));
    }
    if content_len > MAX_CONTENT_LEN {
        return Err(AppError::Validation(format!(
            "content must be at most {MAX_CONTENT_LEN} characters"
        )));
    }
    if input.agent_id.trim().is_empty() {
        return Err(AppError::Validation("agent_id is required".to_owned()));
    }
    if let Some(tags) = &input.tags {
        if tags.len() > MAX_TAGS {
            return Err(AppError::Validation(format!(
                "tags must have at most {MAX_TAGS} entries"
            )));
        }
    }
    if let Some(importance) = input.importance {
        if !(0.0..=1.0).contains(&importance) {
            return Err(AppError::Validation(
                "importance must be between 0.0 and 1.0".to_owned(),
            ));
        }
    }
    Ok(())
}

fn build_payload(input: &ObservationInput) -> serde_json::Value {
    serde_json::json!({
        "content": input.content,
        "agent_id": input.agent_id,
        "user_id": input.user_id,
        "repo": input.repo,
        "tags": input.tags.clone().unwrap_or_default(),
        "importance": input.importance,
        "source_ref": input.source_ref,
        "scope_id": input.scope_id,
    })
}

fn idempotency_key_to_uuid(idempotency_key: &str) -> Uuid {
    Uuid::new_v5(&Uuid::NAMESPACE_URL, idempotency_key.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn idempotency_key_is_deterministic() {
        let workspace_id = Uuid::nil();
        let key_a = idempotency_key(workspace_id, "agent-1", "some content");
        let key_b = idempotency_key(workspace_id, "agent-1", "some content");
        assert_eq!(key_a, key_b);
    }

    #[test]
    fn idempotency_key_differs_by_workspace() {
        let ws_a = Uuid::new_v5(&Uuid::NAMESPACE_URL, b"ws-a");
        let ws_b = Uuid::new_v5(&Uuid::NAMESPACE_URL, b"ws-b");
        let key_a = idempotency_key(ws_a, "agent-1", "same content");
        let key_b = idempotency_key(ws_b, "agent-1", "same content");
        assert_ne!(key_a, key_b);
    }

    #[test]
    fn idempotency_key_differs_by_agent() {
        let ws = Uuid::nil();
        let key_a = idempotency_key(ws, "agent-1", "same content");
        let key_b = idempotency_key(ws, "agent-2", "same content");
        assert_ne!(key_a, key_b);
    }

    #[test]
    fn idempotency_key_differs_by_content() {
        let ws = Uuid::nil();
        let key_a = idempotency_key(ws, "agent-1", "content A");
        let key_b = idempotency_key(ws, "agent-1", "content B");
        assert_ne!(key_a, key_b);
    }

    #[test]
    fn idempotency_key_has_obs_prefix() {
        let key = idempotency_key(Uuid::nil(), "agent-1", "content");
        assert!(key.starts_with("obs:"));
    }

    #[test]
    fn validate_input_rejects_empty_content() {
        let input = ObservationInput {
            content: String::new(),
            agent_id: "agent-1".to_owned(),
            user_id: None,
            repo: None,
            tags: None,
            importance: None,
            source_ref: None,
            scope_id: None,
        };
        assert!(validate_input(&input).is_err());
    }

    #[test]
    fn validate_input_rejects_empty_agent_id() {
        let input = ObservationInput {
            content: "some content".to_owned(),
            agent_id: "   ".to_owned(),
            user_id: None,
            repo: None,
            tags: None,
            importance: None,
            source_ref: None,
            scope_id: None,
        };
        assert!(validate_input(&input).is_err());
    }

    #[test]
    fn validate_input_rejects_out_of_range_importance() {
        let input = ObservationInput {
            content: "some content".to_owned(),
            agent_id: "agent-1".to_owned(),
            user_id: None,
            repo: None,
            tags: None,
            importance: Some(1.5),
            source_ref: None,
            scope_id: None,
        };
        assert!(validate_input(&input).is_err());
    }

    #[test]
    fn validate_input_rejects_too_many_tags() {
        let input = ObservationInput {
            content: "some content".to_owned(),
            agent_id: "agent-1".to_owned(),
            user_id: None,
            repo: None,
            tags: Some(vec!["tag".to_owned(); MAX_TAGS + 1]),
            importance: None,
            source_ref: None,
            scope_id: None,
        };
        assert!(validate_input(&input).is_err());
    }

    #[test]
    fn validate_input_accepts_valid_observation() {
        let input = ObservationInput {
            content: "The deployment completed successfully.".to_owned(),
            agent_id: "deploy-agent".to_owned(),
            user_id: Some("alice".to_owned()),
            repo: Some("org/repo".to_owned()),
            tags: Some(vec!["deploy".to_owned(), "prod".to_owned()]),
            importance: Some(0.75),
            source_ref: Some("run-42".to_owned()),
            scope_id: Some(Uuid::nil()),
        };
        assert!(validate_input(&input).is_ok());
    }
}
