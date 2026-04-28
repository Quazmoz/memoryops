use chrono::{DateTime, Utc};
use common::{models::EventType, AppError};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParsedJiraEvent {
    pub event_type: EventType,
    pub webhook_event: String,
    pub actor: String,
    pub subject_id: String,
    pub occurred_at: DateTime<Utc>,
    pub payload: Value,
}

impl ParsedJiraEvent {
    pub fn idempotency_key(&self) -> String {
        jira_idempotency_key(&self.webhook_event, &self.subject_id, self.occurred_at)
    }
}

pub fn parse_jira_event(body: &Value) -> Result<ParsedJiraEvent, AppError> {
    let webhook_event = required_string(body, "/webhookEvent", "webhookEvent")?;
    let event_type = match webhook_event.as_str() {
        "jira:issue_created" | "jira:issue_updated" | "jira:issue_deleted" => EventType::Issue,
        "comment_created" | "comment_updated" => EventType::IssueComment,
        other => {
            return Err(AppError::Validation(format!(
                "unrecognized Jira event: {other}"
            )))
        }
    };
    let issue = body.get("issue").unwrap_or(&Value::Null);
    let comment = body.get("comment").unwrap_or(&Value::Null);
    let actor = actor_name(body).unwrap_or_else(|| "jira".to_owned());
    let occurred_at = occurred_at(body, issue, comment).unwrap_or_else(Utc::now);
    let issue_key = optional_string(issue, "/key");
    let comment_id = optional_string(comment, "/id");
    let subject_id = comment_id
        .clone()
        .or_else(|| issue_key.clone())
        .or_else(|| optional_string(issue, "/id"))
        .unwrap_or_else(|| payload_hash(body));
    let payload = json!({
        "type": webhook_event.clone(),
        "webhook_event": webhook_event.clone(),
        "issue_key": issue_key,
        "issue_id": optional_string(issue, "/id"),
        "summary": optional_text(issue, "/fields/summary"),
        "description": optional_text(issue, "/fields/description"),
        "status": optional_string(issue, "/fields/status/name"),
        "assignee": optional_string_at(issue, &["/fields/assignee/displayName", "/fields/assignee/name", "/fields/assignee/emailAddress", "/fields/assignee/accountId"]),
        "priority": optional_string(issue, "/fields/priority/name"),
        "project_key": optional_string(issue, "/fields/project/key"),
        "project_name": optional_string(issue, "/fields/project/name"),
        "comment_id": comment_id,
        "body": optional_text(comment, "/body"),
        "actor": actor.clone(),
        "status_changed": status_changed(body),
    });

    Ok(ParsedJiraEvent {
        event_type,
        webhook_event,
        actor,
        subject_id,
        occurred_at,
        payload,
    })
}

pub fn jira_idempotency_key(
    webhook_event: &str,
    subject_id: &str,
    occurred_at: DateTime<Utc>,
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"jira");
    hasher.update(webhook_event.as_bytes());
    hasher.update(subject_id.as_bytes());
    hasher.update(occurred_at.to_rfc3339().as_bytes());
    hex::encode(hasher.finalize())
}

fn actor_name(body: &Value) -> Option<String> {
    optional_string_at(
        body,
        &[
            "/user/displayName",
            "/user/name",
            "/user/emailAddress",
            "/user/accountId",
            "/comment/author/displayName",
            "/comment/author/name",
            "/comment/author/accountId",
        ],
    )
}

fn occurred_at(body: &Value, issue: &Value, comment: &Value) -> Option<DateTime<Utc>> {
    parse_datetime_at(body, &["/timestamp"])
        .or_else(|| parse_datetime_at(comment, &["/updated", "/created"]))
        .or_else(|| parse_datetime_at(issue, &["/fields/updated", "/fields/created"]))
}

fn status_changed(body: &Value) -> bool {
    body.pointer("/changelog/items")
        .and_then(Value::as_array)
        .is_some_and(|items| {
            items.iter().any(|item| {
                optional_string(item, "/field")
                    .or_else(|| optional_string(item, "/fieldId"))
                    .is_some_and(|field| field.eq_ignore_ascii_case("status"))
            })
        })
}

fn parse_datetime_at(body: &Value, pointers: &[&str]) -> Option<DateTime<Utc>> {
    pointers
        .iter()
        .find_map(|pointer| body.pointer(pointer).and_then(parse_datetime_value))
}

fn parse_datetime_value(value: &Value) -> Option<DateTime<Utc>> {
    match value {
        Value::String(raw) => DateTime::parse_from_rfc3339(raw)
            .ok()
            .map(|timestamp| timestamp.with_timezone(&Utc))
            .or_else(|| raw.parse::<i64>().ok().and_then(parse_unix_timestamp)),
        Value::Number(number) => number.as_i64().and_then(parse_unix_timestamp),
        _ => None,
    }
}

fn parse_unix_timestamp(raw: i64) -> Option<DateTime<Utc>> {
    if raw.abs() > 10_000_000_000 {
        DateTime::<Utc>::from_timestamp_millis(raw)
    } else {
        DateTime::<Utc>::from_timestamp(raw, 0)
    }
}

fn required_string(body: &Value, pointer: &str, name: &str) -> Result<String, AppError> {
    optional_string(body, pointer).ok_or_else(|| AppError::Validation(format!("missing {name}")))
}

fn optional_string_at(body: &Value, pointers: &[&str]) -> Option<String> {
    pointers
        .iter()
        .find_map(|pointer| optional_string(body, pointer))
}

fn optional_string(body: &Value, pointer: &str) -> Option<String> {
    body.pointer(pointer)
        .and_then(value_as_string)
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

fn optional_text(body: &Value, pointer: &str) -> Option<String> {
    body.pointer(pointer)
        .and_then(value_as_text)
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

fn value_as_string(value: &Value) -> Option<String> {
    match value {
        Value::String(raw) => Some(raw.to_owned()),
        Value::Number(number) => Some(number.to_string()),
        _ => None,
    }
}

fn value_as_text(value: &Value) -> Option<String> {
    match value {
        Value::String(raw) => Some(raw.to_owned()),
        Value::Null => None,
        other => Some(other.to_string()),
    }
}

fn payload_hash(body: &Value) -> String {
    let mut hasher = Sha256::new();
    hasher.update(body.to_string().as_bytes());
    hex::encode(hasher.finalize())
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;
    use serde_json::json;

    use super::*;

    #[test]
    fn parses_issue_created_event() {
        let payload = json!({
            "webhookEvent": "jira:issue_created",
            "timestamp": 1_777_377_630_000_i64,
            "user": { "displayName": "Ada" },
            "issue": {
                "id": "10001",
                "key": "OPS-123",
                "fields": {
                    "summary": "Fix ingestion",
                    "status": { "name": "To Do" },
                    "priority": { "name": "Critical" },
                    "assignee": { "displayName": "Grace" },
                    "project": { "key": "OPS", "name": "Operations" }
                }
            }
        });

        let parsed = match parse_jira_event(&payload) {
            Ok(parsed) => parsed,
            Err(error) => panic!("Jira issue event should parse: {error}"),
        };
        let expected = match Utc.timestamp_millis_opt(1_777_377_630_000_i64).single() {
            Some(timestamp) => timestamp,
            None => panic!("test timestamp should be valid"),
        };

        assert_eq!(parsed.event_type, EventType::Issue);
        assert_eq!(parsed.actor, "Ada");
        assert_eq!(parsed.occurred_at, expected);
        assert_eq!(parsed.payload["issue_key"], "OPS-123");
        assert_eq!(parsed.payload["priority"], "Critical");
    }

    #[test]
    fn parses_comment_updated_event() {
        let payload = json!({
            "webhookEvent": "comment_updated",
            "user": { "accountId": "user-id" },
            "issue": { "key": "OPS-123" },
            "comment": {
                "id": "comment-id",
                "body": "Updated comment",
                "updated": "2026-04-28T10:20:30Z"
            }
        });

        let parsed = match parse_jira_event(&payload) {
            Ok(parsed) => parsed,
            Err(error) => panic!("Jira comment event should parse: {error}"),
        };

        assert_eq!(parsed.event_type, EventType::IssueComment);
        assert_eq!(parsed.payload["body"], "Updated comment");
        assert_eq!(parsed.payload["comment_id"], "comment-id");
    }

    #[test]
    fn issue_updated_detects_status_change() {
        let payload = json!({
            "webhookEvent": "jira:issue_updated",
            "issue": { "key": "OPS-123", "fields": { "summary": "Fix ingestion" } },
            "changelog": { "items": [{ "field": "status" }] }
        });

        let parsed = match parse_jira_event(&payload) {
            Ok(parsed) => parsed,
            Err(error) => panic!("Jira issue update should parse: {error}"),
        };

        assert_eq!(parsed.payload["status_changed"], true);
    }

    #[test]
    fn unknown_event_returns_validation_error() {
        let payload = json!({ "webhookEvent": "worklog_created" });

        assert!(matches!(
            parse_jira_event(&payload),
            Err(AppError::Validation(_))
        ));
    }
}
