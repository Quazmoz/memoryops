use common::{
    error::AppResult,
    models::{EventType, RawEvent, Source},
};
use redis::aio::ConnectionManager;

pub const STREAM_KEY: &str = "memoryops:raw_events";

pub async fn publish_raw_event(redis: &mut ConnectionManager, event: &RawEvent) -> AppResult<()> {
    let result = redis::cmd("XADD")
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
        .query_async::<()>(redis)
        .await;

    if let Err(error) = result {
        tracing::error!(error = ?error, event_id = %event.id, "failed to publish raw event");
    }

    Ok(())
}

fn source_as_str(source: Source) -> &'static str {
    match source {
        Source::GitHub => "github",
        Source::Slack => "slack",
        Source::Jira => "jira",
        Source::Linear => "linear",
        Source::Observation => "observation",
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
        EventType::AgentObservation => "agent_observation",
    }
}

#[cfg(test)]
mod tests {
    use chrono::Utc;
    use serde_json::json;
    use uuid::Uuid;

    use super::*;

    fn raw_event() -> RawEvent {
        RawEvent {
            id: Uuid::now_v7(),
            workspace_id: Uuid::now_v7(),
            source: Source::GitHub,
            event_type: EventType::PullRequest,
            actor: "octocat".to_owned(),
            payload: json!({}),
            idempotency_key: "github:test-delivery".to_owned(),
            occurred_at: Utc::now(),
            ingested_at: Utc::now(),
        }
    }

    #[test]
    fn stream_key_is_stable() {
        assert_eq!(STREAM_KEY, "memoryops:raw_events");
    }

    #[test]
    fn queue_field_values_match_database_enum_names() {
        assert_eq!(source_as_str(Source::GitHub), "github");
        assert_eq!(source_as_str(Source::Observation), "observation");
        assert_eq!(event_type_as_str(EventType::PullRequest), "pull_request");
        assert_eq!(
            event_type_as_str(EventType::AgentObservation),
            "agent_observation"
        );
    }

    #[tokio::test]
    #[ignore = "requires a live Redis instance because ConnectionManager opens a TCP connection"]
    async fn publish_raw_event_to_live_redis_stream() {
        let redis_url =
            std::env::var("REDIS_URL").unwrap_or_else(|_| "redis://localhost:16379".to_owned());
        let client = match redis::Client::open(redis_url) {
            Ok(client) => client,
            Err(error) => panic!("test Redis URL should be valid: {error}"),
        };
        let mut redis = match ConnectionManager::new(client).await {
            Ok(connection) => connection,
            Err(error) => panic!("test Redis should be reachable: {error}"),
        };

        if let Err(error) = publish_raw_event(&mut redis, &raw_event()).await {
            panic!("publish should not fail the caller: {error}");
        }
    }
}
