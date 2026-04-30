use std::collections::BTreeSet;

use common::{
    error::AppResult,
    models::{Entity, EntityType, EventType, RawEvent, Source},
};
use serde_json::Value;

const NO_CONTENT: &str = "(no content)";

pub fn extract_text(event: &RawEvent) -> AppResult<String> {
    let content = match event.event_type {
        EventType::AgentObservation => {
            string_or_no_content(event.payload.get("content"))
        }
        EventType::PullRequest => format!(
            "{}\n\n{}",
            string_or_no_content(event.payload.pointer("/pull_request/title")),
            string_or_no_content(event.payload.pointer("/pull_request/body"))
        ),
        EventType::PullRequestReview => string_or_no_content(event.payload.pointer("/review/body")),
        EventType::Push => extract_push_text(&event.payload),
        EventType::IssueComment => match event.source {
            Source::Jira => extract_jira_comment_text(&event.payload),
            Source::Linear => extract_linear_comment_text(&event.payload),
            Source::GitHub | Source::Slack | Source::Observation => {
                string_or_no_content(event.payload.pointer("/comment/body"))
            }
        },
        EventType::Issue => match event.source {
            Source::Jira => extract_jira_issue_text(&event.payload),
            Source::Linear => extract_linear_issue_text(&event.payload),
            Source::GitHub | Source::Slack | Source::Observation => format!(
                "{}\n\n{}",
                string_or_no_content(event.payload.pointer("/issue/title")),
                string_or_no_content(event.payload.pointer("/issue/body"))
            ),
        },
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
        Source::Jira => extract_jira_entities(&event.payload, content),
        Source::Linear => extract_linear_entities(&event.payload, content),
        Source::GitHub | Source::Observation => Vec::new(),
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

fn extract_linear_issue_text(payload: &Value) -> String {
    let object_type =
        string_or_no_content(payload.get("object_type").or_else(|| payload.get("type")));
    let action = string_or_no_content(payload.get("action"));
    let identifier = string_or_no_content(payload.get("identifier"));
    let title = string_or_no_content(payload.get("title"));
    let body = string_or_no_content(payload.get("body"));
    let status = string_or_no_content(payload.get("status"));
    let assignee = string_or_no_content(payload.get("assignee"));
    let priority = string_or_no_content(payload.get("priority"));

    format!(
        "Linear {object_type} {action}: {identifier} {title}\nStatus: {status}\nAssignee: {assignee}\nPriority: {priority}\n\n{body}"
    )
}

fn extract_linear_comment_text(payload: &Value) -> String {
    let issue = string_or_no_content(payload.pointer("/issue/identifier"));
    let body = string_or_no_content(payload.get("body"));
    let actor = string_or_no_content(payload.get("actor"));

    format!("Linear comment by {actor} on {issue}: {body}")
}

fn extract_jira_issue_text(payload: &Value) -> String {
    let issue_key = string_or_no_content(payload.get("issue_key"));
    let summary = string_or_no_content(payload.get("summary"));
    let description = string_or_no_content(payload.get("description"));
    let status = string_or_no_content(payload.get("status"));
    let assignee = string_or_no_content(payload.get("assignee"));
    let priority = string_or_no_content(payload.get("priority"));

    format!(
        "Jira issue {issue_key}: {summary}\nStatus: {status}\nAssignee: {assignee}\nPriority: {priority}\n\n{description}"
    )
}

fn extract_jira_comment_text(payload: &Value) -> String {
    let issue_key = string_or_no_content(payload.get("issue_key"));
    let body = string_or_no_content(payload.get("body"));
    let actor = string_or_no_content(payload.get("actor"));

    format!("Jira comment by {actor} on {issue_key}: {body}")
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

fn extract_linear_entities(payload: &Value, content: &str) -> Vec<Entity> {
    let mut entities = Vec::new();

    add_optional_entity(
        &mut entities,
        EntityType::Person,
        payload.get("actor"),
        0.85,
    );
    add_optional_entity(
        &mut entities,
        EntityType::Person,
        payload.get("assignee"),
        0.85,
    );
    add_optional_entity(&mut entities, EntityType::Team, payload.get("team"), 0.90);
    add_optional_entity(
        &mut entities,
        EntityType::Topic,
        payload.get("identifier"),
        0.90,
    );
    add_optional_entity(
        &mut entities,
        EntityType::Topic,
        payload.pointer("/issue/identifier"),
        0.90,
    );
    add_optional_entity(
        &mut entities,
        EntityType::Topic,
        payload.get("project"),
        0.80,
    );
    add_optional_entity(&mut entities, EntityType::Topic, payload.get("cycle"), 0.80);
    add_optional_entity(
        &mut entities,
        EntityType::Topic,
        payload.get("status"),
        0.70,
    );
    add_optional_entity(
        &mut entities,
        EntityType::Topic,
        payload.get("priority"),
        0.70,
    );

    for value in github_repo_mentions(content) {
        add_entity(&mut entities, EntityType::Repo, value, 0.80);
    }

    entities
}

fn extract_jira_entities(payload: &Value, content: &str) -> Vec<Entity> {
    let mut entities = Vec::new();

    add_optional_entity(
        &mut entities,
        EntityType::Person,
        payload.get("actor"),
        0.85,
    );
    add_optional_entity(
        &mut entities,
        EntityType::Person,
        payload.get("assignee"),
        0.85,
    );
    add_optional_entity(
        &mut entities,
        EntityType::Team,
        payload.get("project_key"),
        0.90,
    );
    add_optional_entity(
        &mut entities,
        EntityType::Team,
        payload.get("project_name"),
        0.75,
    );
    add_optional_entity(
        &mut entities,
        EntityType::Topic,
        payload.get("issue_key"),
        0.90,
    );
    add_optional_entity(
        &mut entities,
        EntityType::Topic,
        payload.get("status"),
        0.75,
    );
    add_optional_entity(
        &mut entities,
        EntityType::Topic,
        payload.get("priority"),
        0.75,
    );

    for value in github_repo_mentions(content) {
        add_entity(&mut entities, EntityType::Repo, value, 0.80);
    }

    entities
}

fn add_optional_entity(
    entities: &mut Vec<Entity>,
    entity_type: EntityType,
    value: Option<&Value>,
    confidence: f32,
) {
    if let Some(value) = value.and_then(value_as_entity_string) {
        add_entity(entities, entity_type, value, confidence);
    }
}

fn add_entity(entities: &mut Vec<Entity>, entity_type: EntityType, value: String, confidence: f32) {
    if entities
        .iter()
        .any(|entity| entity.entity_type == entity_type && entity.value == value)
    {
        return;
    }

    entities.push(Entity {
        entity_type,
        value,
        confidence,
    });
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

fn value_as_entity_string(value: &Value) -> Option<String> {
    match value {
        Value::String(raw) => Some(raw.trim().to_owned()).filter(|value| !value.is_empty()),
        Value::Number(number) => Some(number.to_string()),
        _ => None,
    }
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

    #[test]
    fn extract_linear_issue_content_and_entities() {
        let event = RawEvent {
            id: Uuid::now_v7(),
            workspace_id: Uuid::now_v7(),
            source: Source::Linear,
            event_type: EventType::Issue,
            actor: "Ada".to_owned(),
            payload: json!({
                "type": "Issue",
                "object_type": "Issue",
                "action": "create",
                "identifier": "OPS-123",
                "title": "Fix ingestion",
                "status": "Todo",
                "assignee": "Grace",
                "priority": "High",
                "team": "OPS",
                "body": "See https://github.com/Quazmoz/memoryops/issues/1"
            }),
            idempotency_key: "linear:test".to_owned(),
            occurred_at: Utc::now(),
            ingested_at: Utc::now(),
        };

        let text = match extract_text(&event) {
            Ok(text) => text,
            Err(error) => panic!("Linear issue text should extract: {error}"),
        };
        let entities = extract_entities(&event, &text);

        assert!(text.contains("OPS-123"));
        assert!(entities
            .iter()
            .any(|entity| { entity.entity_type == EntityType::Person && entity.value == "Grace" }));
        assert!(entities
            .iter()
            .any(|entity| { entity.entity_type == EntityType::Team && entity.value == "OPS" }));
        assert!(entities.iter().any(|entity| {
            entity.entity_type == EntityType::Repo && entity.value == "Quazmoz/memoryops"
        }));
    }

    #[test]
    fn extract_jira_issue_content_and_entities() {
        let event = RawEvent {
            id: Uuid::now_v7(),
            workspace_id: Uuid::now_v7(),
            source: Source::Jira,
            event_type: EventType::Issue,
            actor: "Ada".to_owned(),
            payload: json!({
                "type": "jira:issue_created",
                "issue_key": "OPS-123",
                "summary": "Fix ingestion",
                "status": "To Do",
                "assignee": "Grace",
                "priority": "Critical",
                "project_key": "OPS",
                "project_name": "Operations",
                "description": "See https://github.com/Quazmoz/memoryops/issues/1",
                "actor": "Ada"
            }),
            idempotency_key: "jira:test".to_owned(),
            occurred_at: Utc::now(),
            ingested_at: Utc::now(),
        };

        let text = match extract_text(&event) {
            Ok(text) => text,
            Err(error) => panic!("Jira issue text should extract: {error}"),
        };
        let entities = extract_entities(&event, &text);

        assert!(text.contains("OPS-123"));
        assert!(entities
            .iter()
            .any(|entity| { entity.entity_type == EntityType::Person && entity.value == "Grace" }));
        assert!(entities
            .iter()
            .any(|entity| { entity.entity_type == EntityType::Team && entity.value == "OPS" }));
        assert!(entities.iter().any(|entity| {
            entity.entity_type == EntityType::Repo && entity.value == "Quazmoz/memoryops"
        }));
    }
}
