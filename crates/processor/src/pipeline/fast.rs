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
    match event.source {
        Source::Slack => slack_importance_score(event),
        Source::Jira => jira_importance_score(event),
        Source::Linear => linear_importance_score(event),
        Source::GitHub => {
            (source_authority_weight(state, event.source) * 0.5)
                + (event_type_base_importance(event.event_type) * 0.5)
        }
    }
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

fn jira_importance_score(event: &RawEvent) -> f32 {
    match event.payload.get("type").and_then(Value::as_str) {
        Some("jira:issue_created") if priority_is_high_or_critical(&event.payload) => 0.80,
        Some("jira:issue_created") => 0.45,
        Some("jira:issue_updated")
            if event
                .payload
                .get("status_changed")
                .and_then(Value::as_bool)
                .unwrap_or(false) =>
        {
            0.55
        }
        Some("jira:issue_updated") => 0.45,
        Some("jira:issue_deleted") => 0.50,
        Some("comment_created") | Some("comment_updated") => 0.35,
        _ => match event.event_type {
            EventType::Issue => 0.45,
            EventType::IssueComment => 0.35,
            _ => 0.30,
        },
    }
}

fn linear_importance_score(event: &RawEvent) -> f32 {
    let object_type = event
        .payload
        .get("object_type")
        .or_else(|| event.payload.get("type"))
        .and_then(Value::as_str)
        .map(str::to_ascii_lowercase);

    match object_type.as_deref() {
        Some("issue") if priority_equals(&event.payload, "urgent") => 0.80,
        Some("issue") if priority_equals(&event.payload, "high") => 0.65,
        Some("issue") => 0.45,
        Some("comment") => 0.40,
        Some("project") | Some("cycle") => 0.50,
        _ => match event.event_type {
            EventType::Issue => 0.45,
            EventType::IssueComment => 0.40,
            _ => 0.30,
        },
    }
}

fn priority_is_high_or_critical(payload: &Value) -> bool {
    payload
        .get("priority")
        .and_then(Value::as_str)
        .is_some_and(|priority| {
            matches!(
                priority.to_ascii_lowercase().as_str(),
                "high" | "critical" | "highest" | "blocker"
            )
        })
}

fn priority_equals(payload: &Value, expected: &str) -> bool {
    payload
        .get("priority")
        .and_then(Value::as_str)
        .is_some_and(|priority| priority.eq_ignore_ascii_case(expected))
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

    #[test]
    fn jira_importance_uses_event_specific_rules() {
        assert_eq!(
            jira_importance_score(&source_event(
                Source::Jira,
                EventType::Issue,
                json!({ "type": "jira:issue_created", "priority": "Critical" })
            )),
            0.80
        );
        assert_eq!(
            jira_importance_score(&source_event(
                Source::Jira,
                EventType::Issue,
                json!({ "type": "jira:issue_created", "priority": "Normal" })
            )),
            0.45
        );
        assert_eq!(
            jira_importance_score(&source_event(
                Source::Jira,
                EventType::Issue,
                json!({ "type": "jira:issue_updated", "status_changed": true })
            )),
            0.55
        );
    }

    #[test]
    fn linear_importance_uses_event_specific_rules() {
        assert_eq!(
            linear_importance_score(&source_event(
                Source::Linear,
                EventType::Issue,
                json!({ "type": "Issue", "priority": "Urgent" })
            )),
            0.80
        );
        assert_eq!(
            linear_importance_score(&source_event(
                Source::Linear,
                EventType::Issue,
                json!({ "type": "Issue", "priority": "High" })
            )),
            0.65
        );
        assert_eq!(
            linear_importance_score(&source_event(
                Source::Linear,
                EventType::IssueComment,
                json!({ "type": "Comment" })
            )),
            0.40
        );
        assert_eq!(
            linear_importance_score(&source_event(
                Source::Linear,
                EventType::Issue,
                json!({ "type": "Project", "action": "update" })
            )),
            0.50
        );
    }

    fn source_event(source: Source, event_type: EventType, payload: serde_json::Value) -> RawEvent {
        RawEvent {
            id: Uuid::now_v7(),
            workspace_id: Uuid::now_v7(),
            source,
            event_type,
            actor: "actor".to_owned(),
            payload,
            idempotency_key: "source:test".to_owned(),
            occurred_at: Utc::now(),
            ingested_at: Utc::now(),
        }
    }
}
