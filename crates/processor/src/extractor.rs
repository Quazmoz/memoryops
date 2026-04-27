use common::{
    error::AppResult,
    models::{EventType, RawEvent},
};
use serde_json::Value;

const NO_CONTENT: &str = "(no content)";

pub fn extract_text(event: &RawEvent) -> AppResult<String> {
    let content = match event.event_type {
        EventType::PullRequest => format!(
            "{}\n\n{}",
            string_or_no_content(event.payload.pointer("/pull_request/title")),
            string_or_no_content(event.payload.pointer("/pull_request/body"))
        ),
        EventType::PullRequestReview => string_or_no_content(event.payload.pointer("/review/body")),
        EventType::Push => extract_push_text(&event.payload),
        EventType::IssueComment => string_or_no_content(event.payload.pointer("/comment/body")),
        EventType::Issue => format!(
            "{}\n\n{}",
            string_or_no_content(event.payload.pointer("/issue/title")),
            string_or_no_content(event.payload.pointer("/issue/body"))
        ),
        EventType::Message | EventType::Reaction => string_or_no_content(event.payload.get("text")),
    };

    if content.trim().is_empty() {
        Ok(NO_CONTENT.to_owned())
    } else {
        Ok(content)
    }
}

fn extract_push_text(payload: &Value) -> String {
    let commits = payload.get("commits").and_then(Value::as_array);
    let commit_count = commits.map_or(0, Vec::len);
    let first_commit_message = commits
        .and_then(|items| items.first())
        .and_then(|commit| commit.get("message"));
    let reference = string_or_no_content(payload.get("ref"));
    let message = string_or_no_content(first_commit_message);

    format!("Pushed {commit_count} commits to {reference}: {message}")
}

fn string_or_no_content(value: Option<&Value>) -> String {
    value
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| NO_CONTENT.to_owned())
}

#[cfg(test)]
mod tests {
    use chrono::Utc;
    use common::models::Source;
    use serde_json::json;
    use uuid::Uuid;

    use super::*;

    fn raw_event(event_type: EventType, payload: serde_json::Value) -> RawEvent {
        RawEvent {
            id: Uuid::now_v7(),
            workspace_id: Uuid::now_v7(),
            source: Source::GitHub,
            event_type,
            actor: "octocat".to_owned(),
            payload,
            idempotency_key: "github:test".to_owned(),
            occurred_at: Utc::now(),
            ingested_at: Utc::now(),
        }
    }

    #[test]
    fn extract_pull_request_content() {
        let event = raw_event(
            EventType::PullRequest,
            json!({ "pull_request": { "title": "Add processor", "body": "Build workers" } }),
        );

        let text = match extract_text(&event) {
            Ok(text) => text,
            Err(error) => panic!("pull request text should extract: {error}"),
        };

        assert_eq!(text, "Add processor\n\nBuild workers");
    }

    #[test]
    fn extract_push_content() {
        let event = raw_event(
            EventType::Push,
            json!({
                "ref": "refs/heads/main",
                "commits": [{ "message": "Implement queue consumer" }]
            }),
        );

        let text = match extract_text(&event) {
            Ok(text) => text,
            Err(error) => panic!("push text should extract: {error}"),
        };

        assert_eq!(
            text,
            "Pushed 1 commits to refs/heads/main: Implement queue consumer"
        );
    }

    #[test]
    fn missing_body_falls_back_to_no_content() {
        let event = raw_event(
            EventType::PullRequest,
            json!({ "pull_request": { "title": "Add processor", "body": null } }),
        );

        let text = match extract_text(&event) {
            Ok(text) => text,
            Err(error) => panic!("pull request text should extract: {error}"),
        };

        assert_eq!(text, "Add processor\n\n(no content)");
    }

    #[test]
    fn empty_string_body_falls_back_to_no_content() {
        let event = raw_event(
            EventType::PullRequest,
            json!({ "pull_request": { "title": "Add processor", "body": "" } }),
        );

        let text = match extract_text(&event) {
            Ok(text) => text,
            Err(error) => panic!("pull request text should extract: {error}"),
        };

        assert_eq!(text, "Add processor\n\n(no content)");
    }
}
