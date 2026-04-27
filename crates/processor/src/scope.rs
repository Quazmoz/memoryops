use common::models::{RawEvent, Source};
use serde_json::{json, Value};

pub fn build_scope(event: &RawEvent) -> Value {
    json!({
        "workspace_id": event.workspace_id,
        "source": source_as_str(event.source),
        "repo": repo_name(event),
        "actor": event.actor,
        "agent_id": Value::Null,
        "user_id": Value::Null,
    })
}

fn repo_name(event: &RawEvent) -> Value {
    event
        .payload
        .pointer("/repository/full_name")
        .and_then(Value::as_str)
        .map(|repo| json!(repo))
        .unwrap_or(Value::Null)
}

fn source_as_str(source: Source) -> &'static str {
    match source {
        Source::GitHub => "github",
        Source::Slack => "slack",
        Source::Jira => "jira",
        Source::Linear => "linear",
    }
}

#[cfg(test)]
mod tests {
    use chrono::Utc;
    use common::models::EventType;
    use serde_json::json;
    use uuid::Uuid;

    use super::*;

    fn raw_event(payload: Value) -> RawEvent {
        RawEvent {
            id: Uuid::now_v7(),
            workspace_id: Uuid::now_v7(),
            source: Source::GitHub,
            event_type: EventType::PullRequest,
            actor: "octocat".to_owned(),
            payload,
            idempotency_key: "github:test".to_owned(),
            occurred_at: Utc::now(),
            ingested_at: Utc::now(),
        }
    }

    #[test]
    fn github_scope_includes_repo_and_actor() {
        let event = raw_event(json!({ "repository": { "full_name": "Quazmoz/memoryops" } }));

        let scope = build_scope(&event);

        assert_eq!(scope["source"], "github");
        assert_eq!(scope["repo"], "Quazmoz/memoryops");
        assert_eq!(scope["actor"], "octocat");
    }

    #[test]
    fn missing_repo_field_produces_null() {
        let event = raw_event(json!({}));

        let scope = build_scope(&event);

        assert!(scope["repo"].is_null());
    }
}
