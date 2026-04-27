use common::models::{EventType, RawEvent};

pub fn should_use_fast_path(event: &RawEvent) -> bool {
    matches!(
        event.event_type,
        EventType::Push | EventType::PullRequestReview
    )
}

#[cfg(test)]
mod tests {
    use chrono::Utc;
    use common::models::Source;
    use serde_json::json;
    use uuid::Uuid;

    use super::*;

    fn raw_event(event_type: EventType) -> RawEvent {
        RawEvent {
            id: Uuid::now_v7(),
            workspace_id: Uuid::now_v7(),
            source: Source::GitHub,
            event_type,
            actor: "octocat".to_owned(),
            payload: json!({}),
            idempotency_key: "github:test".to_owned(),
            occurred_at: Utc::now(),
            ingested_at: Utc::now(),
        }
    }

    #[test]
    fn push_is_fast_path() {
        assert!(should_use_fast_path(&raw_event(EventType::Push)));
    }

    #[test]
    fn pull_request_review_is_fast_path() {
        assert!(should_use_fast_path(&raw_event(
            EventType::PullRequestReview
        )));
    }

    #[test]
    fn pull_request_is_slow_path() {
        assert!(!should_use_fast_path(&raw_event(EventType::PullRequest)));
    }

    #[test]
    fn issue_comment_is_slow_path() {
        assert!(!should_use_fast_path(&raw_event(EventType::IssueComment)));
    }
}
