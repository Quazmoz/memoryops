use std::collections::BTreeSet;

use common::{
    error::AppResult,
    models::{Entity, EntityType, EventType, RawEvent, Source},
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
        EventType::Message => string_or_no_content(event.payload.get("text")),
        EventType::Reaction => extract_reaction_text(&event.payload),
    };

    if content.trim().is_empty() {
        Ok(NO_CONTENT.to_owned())
    } else {
        Ok(content)
    }
}

pub fn extract_entities(event: &RawEvent, content: &str) -> Vec<Entity> {
    match event.source {
        Source::Slack => extract_slack_entities(content),
        Source::GitHub | Source::Jira | Source::Linear => Vec::new(),
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

fn extract_reaction_text(payload: &Value) -> String {
    let reaction = string_or_no_content(payload.get("reaction"));
    let user = string_or_no_content(payload.get("user"));
    let channel = string_or_no_content(
        payload
            .get("channel")
            .or_else(|| payload.pointer("/item/channel")),
    );
    let timestamp = string_or_no_content(payload.get("ts").or_else(|| payload.pointer("/item/ts")));

    format!("Reaction :{reaction}: by {user} on message {channel}/{timestamp}")
}

fn extract_slack_entities(content: &str) -> Vec<Entity> {
    let mut entities = Vec::new();

    for value in person_mentions(content) {
        entities.push(Entity {
            entity_type: EntityType::Person,
            value,
            confidence: 0.90,
        });
    }
    for value in topic_mentions(content) {
        entities.push(Entity {
            entity_type: EntityType::Topic,
            value,
            confidence: 0.80,
        });
    }
    for value in github_repo_mentions(content) {
        entities.push(Entity {
            entity_type: EntityType::Repo,
            value,
            confidence: 0.95,
        });
    }

    entities
}

fn person_mentions(content: &str) -> BTreeSet<String> {
    content
        .split_whitespace()
        .filter_map(|token| {
            normalized_token(token)
                .strip_prefix('@')
                .map(ToOwned::to_owned)
        })
        .filter(|value| is_identifier(value))
        .collect()
}

fn topic_mentions(content: &str) -> BTreeSet<String> {
    content
        .split_whitespace()
        .filter_map(|token| {
            let normalized = normalized_token(token);
            let channel = normalized.strip_prefix('#')?;
            let display_name = channel.split_once('|').map_or(channel, |(_, name)| name);
            if is_channel_name(display_name) {
                Some(format!("#{display_name}"))
            } else {
                None
            }
        })
        .collect()
}

fn github_repo_mentions(content: &str) -> BTreeSet<String> {
    content
        .split_whitespace()
        .filter_map(|token| {
            let normalized = normalized_token(token);
            let start = normalized.find("github.com/")? + "github.com/".len();
            let path = &normalized[start..];
            let mut segments = path.split('/');
            let org = clean_repo_segment(segments.next()?);
            let repo = clean_repo_segment(segments.next()?);
            if is_repo_segment(org) && is_repo_segment(repo) {
                Some(format!("{org}/{repo}"))
            } else {
                None
            }
        })
        .collect()
}

fn normalized_token(token: &str) -> &str {
    token.trim_matches(|character: char| {
        matches!(
            character,
            '<' | '>'
                | ','
                | '.'
                | ';'
                | ':'
                | '!'
                | '?'
                | ')'
                | '('
                | '['
                | ']'
                | '{'
                | '}'
                | '"'
                | '\''
        )
    })
}

fn clean_repo_segment(segment: &str) -> &str {
    segment
        .trim_matches(|character: char| {
            matches!(
                character,
                ',' | '.' | ';' | ':' | '!' | '?' | ')' | '(' | '[' | ']' | '{' | '}' | '"' | '\''
            )
        })
        .strip_suffix(".git")
        .unwrap_or_else(|| {
            segment.trim_matches(|character: char| {
                matches!(
                    character,
                    ',' | '.'
                        | ';'
                        | ':'
                        | '!'
                        | '?'
                        | ')'
                        | '('
                        | '['
                        | ']'
                        | '{'
                        | '}'
                        | '"'
                        | '\''
                )
            })
        })
}

fn is_identifier(value: &str) -> bool {
    !value.is_empty()
        && value
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '_' | '-'))
}

fn is_channel_name(value: &str) -> bool {
    !value.is_empty()
        && value
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '_' | '-'))
}

fn is_repo_segment(value: &str) -> bool {
    !value.is_empty()
        && value.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '_' | '-' | '.')
        })
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

    #[test]
    fn extract_slack_reaction_content() {
        let event = RawEvent {
            id: Uuid::now_v7(),
            workspace_id: Uuid::now_v7(),
            source: Source::Slack,
            event_type: EventType::Reaction,
            actor: "U123".to_owned(),
            payload: json!({
                "type": "reaction_added",
                "user": "U123",
                "reaction": "eyes",
                "item": { "channel": "C123", "ts": "1712345678.123456" }
            }),
            idempotency_key: "slack:test".to_owned(),
            occurred_at: Utc::now(),
            ingested_at: Utc::now(),
        };

        let text = match extract_text(&event) {
            Ok(text) => text,
            Err(error) => panic!("reaction text should extract: {error}"),
        };

        assert_eq!(
            text,
            "Reaction :eyes: by U123 on message C123/1712345678.123456"
        );
    }

    #[test]
    fn extract_slack_entities_from_text() {
        let entities = extract_slack_entities(
            "<@U123> check #platform and https://github.com/Quazmoz/memoryops/pull/1",
        );

        assert!(entities
            .iter()
            .any(|entity| { entity.entity_type == EntityType::Person && entity.value == "U123" }));
        assert!(entities.iter().any(|entity| {
            entity.entity_type == EntityType::Topic && entity.value == "#platform"
        }));
        assert!(entities.iter().any(|entity| {
            entity.entity_type == EntityType::Repo && entity.value == "Quazmoz/memoryops"
        }));
    }
}
