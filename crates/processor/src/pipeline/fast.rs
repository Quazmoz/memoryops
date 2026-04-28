use anyhow::anyhow;
use common::{
    error::AppResult,
    models::{EventType, MemoryType, MemoryUnit, RawEvent, Source},
    AppError, AppState,
};
use serde_json::Value;
use uuid::Uuid;

use crate::{
    extractor, scope,
    store::{self, NewMemoryUnit},
};

pub async fn run_fast_path(state: &AppState, event: &RawEvent) -> AppResult<MemoryUnit> {
    let content = extractor::extract_text(event)?;
    let entities = serde_json::to_value(extractor::extract_entities(event, &content))
        .map_err(|error| AppError::Internal(anyhow!(error)))?;
    let memory_id = Uuid::now_v7();
    let token_count = count_tokens(&content)?;

    let unit = NewMemoryUnit {
        id: memory_id,
        workspace_id: event.workspace_id,
        scope: scope::build_scope(event),
        memory_type: MemoryType::Episodic,
        content,
        entities,
        importance_score: importance_score(state, event),
        source_events: vec![event.id],
        embedding_id: None,
        token_count: Some(token_count),
        tags: Vec::new(),
    };

    store::insert_memory_unit(&state.db, &unit).await
}

pub(crate) fn importance_score(state: &AppState, event: &RawEvent) -> f32 {
    if event.source == Source::Slack {
        return slack_importance_score(event);
    }

    (source_authority_weight(state, event.source) * 0.5)
        + (event_type_base_importance(event.event_type) * 0.5)
}

pub(crate) fn count_tokens(content: &str) -> AppResult<i32> {
    let tokenizer = tiktoken_rs::cl100k_base()
        .map_err(|error| AppError::Internal(anyhow!("failed to initialize tokenizer: {error}")))?;
    let token_count = tokenizer.encode_with_special_tokens(content).len();

    i32::try_from(token_count)
        .map_err(|error| AppError::Internal(anyhow!("token count exceeded i32 range: {error}")))
}

fn source_authority_weight(state: &AppState, source: Source) -> f32 {
    let authority = &state.config.retrieval.source_authority;
    match source {
        Source::GitHub => authority.github,
        Source::Slack => authority.slack,
        Source::Jira => authority.jira,
        Source::Linear => authority.linear,
    }
}

fn event_type_base_importance(event_type: EventType) -> f32 {
    match event_type {
        EventType::PullRequest => 0.8,
        EventType::PullRequestReview => 0.6,
        EventType::Push => 0.4,
        EventType::IssueComment => 0.5,
        EventType::Issue => 0.7,
        EventType::Message | EventType::Reaction => 0.3,
    }
}

fn slack_importance_score(event: &RawEvent) -> f32 {
    match event.payload.get("type").and_then(Value::as_str) {
        Some("app_mention") => 0.70,
        Some("reaction_added") => 0.10,
        Some("message") | Some("message.edited") => slack_message_importance(event),
        _ => match event.event_type {
            EventType::Reaction => 0.10,
            EventType::Message => slack_message_importance(event),
            _ => 0.30,
        },
    }
}

fn slack_message_importance(event: &RawEvent) -> f32 {
    if event
        .payload
        .get("thread_ts")
        .and_then(Value::as_str)
        .filter(|thread_ts| !thread_ts.is_empty())
        .is_some()
    {
        0.40
    } else {
        0.30
    }
}

#[cfg(test)]
mod tests {
    use chrono::Utc;
    use serde_json::json;

    use super::*;

    fn slack_event(event_type: EventType, payload: serde_json::Value) -> RawEvent {
        RawEvent {
            id: Uuid::now_v7(),
            workspace_id: Uuid::now_v7(),
            source: Source::Slack,
            event_type,
            actor: "U123".to_owned(),
            payload,
            idempotency_key: "slack:test".to_owned(),
            occurred_at: Utc::now(),
            ingested_at: Utc::now(),
        }
    }

    #[test]
    fn slack_importance_uses_event_specific_rules() {
        assert_eq!(
            slack_importance_score(&slack_event(
                EventType::Message,
                json!({ "type": "app_mention" })
            )),
            0.70
        );
        assert_eq!(
            slack_importance_score(&slack_event(
                EventType::Message,
                json!({ "type": "message", "thread_ts": "1712345678.123456" })
            )),
            0.40
        );
        assert_eq!(
            slack_importance_score(&slack_event(
                EventType::Message,
                json!({ "type": "message" })
            )),
            0.30
        );
        assert_eq!(
            slack_importance_score(&slack_event(
                EventType::Reaction,
                json!({ "type": "reaction_added" })
            )),
            0.10
        );
    }
}
