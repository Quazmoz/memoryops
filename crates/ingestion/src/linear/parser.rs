use chrono::{DateTime, Utc};
use common::{models::EventType, AppError};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParsedLinearEvent {
    pub event_type: EventType,
    pub event_kind: String,
    pub actor: String,
    pub subject_id: String,
    pub occurred_at: DateTime<Utc>,
    pub payload: Value,
}

impl ParsedLinearEvent {
    pub fn idempotency_key(&self) -> String {
        linear_idempotency_key(&self.event_kind, &self.subject_id, self.occurred_at)
    }
}

pub fn parse_linear_event(body: &Value) -> Result<ParsedLinearEvent, AppError> {
    let object_type = required_string(body, "/type", "type")?;
    let action = required_string(body, "/action", "action")?;
    let object_type_lower = object_type.to_ascii_lowercase();
    let event_type = match object_type_lower.as_str() {
        "issue" | "project" | "cycle" => EventType::Issue,
        "comment" => EventType::IssueComment,
        other => {
            return Err(AppError::Validation(format!(
                "unrecognized Linear event type: {other}"
            )))
        }
    };
    let data = body.get("data").unwrap_or(body);
    let actor = actor_name(body).unwrap_or_else(|| "linear".to_owned());
    let occurred_at = occurred_at(body, data).unwrap_or_else(Utc::now);
    let event_kind = format!("linear.{object_type_lower}.{action}");
    let subject_id = subject_id(data).unwrap_or_else(|| payload_hash(body));
    let issue_identifier = optional_string_at(data, &["/issue/identifier", "/issue/key"]);
    let payload = json!({
        "type": object_type.clone(),
        "object_type": object_type,
        "action": action,
        "event_kind": event_kind.clone(),
        "id": optional_string(data, "/id"),
        "identifier": optional_string_at(data, &["/identifier", "/number", "/key"]),
        "title": optional_string_at(data, &["/title", "/name"]),
        "body": optional_text_at(data, &["/body", "/description"]),
        "priority": priority_label(data),
        "status": optional_string_at(data, &["/state/name", "/status/name", "/status"]),
        "assignee": optional_string_at(data, &["/assignee/name", "/assignee/displayName", "/assignee/email", "/assignee/id"]),
        "team": optional_string_at(data, &["/team/key", "/team/name", "/team/id"]),
        "project": optional_string_at(data, &["/project/name", "/project/id"]),
        "cycle": optional_string_at(data, &["/cycle/name", "/cycle/id"]),
        "url": optional_string(data, "/url"),
        "issue": {
            "identifier": issue_identifier,
            "title": optional_string(data, "/issue/title")
        },
        "actor": actor.clone(),
    });

    Ok(ParsedLinearEvent {
        event_type,
        event_kind,
        actor,
        subject_id,
        occurred_at,
        payload,
    })
}

pub fn linear_idempotency_key(
    event_kind: &str,
    subject_id: &str,
    occurred_at: DateTime<Utc>,
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"linear");
    hasher.update(event_kind.as_bytes());
    hasher.update(subject_id.as_bytes());
    hasher.update(occurred_at.to_rfc3339().as_bytes());
    hex::encode(hasher.finalize())
}

fn actor_name(body: &Value) -> Option<String> {
    optional_string_at(
        body,
        &[
            "/actor/name",
            "/actor/displayName",
            "/actor/email",
            "/actor/id",
            "/user/name",
            "/user/id",
        ],
    )
}

fn subject_id(data: &Value) -> Option<String> {
    optional_string_at(
        data,
        &[
            "/id",
            "/identifier",
            "/key",
            "/number",
            "/issue/id",
            "/issue/identifier",
        ],
    )
}

fn priority_label(data: &Value) -> Option<String> {
    optional_string_at(
        data,
        &["/priorityLabel", "/priority/name", "/priority/label"],
    )
    .or_else(|| data.pointer("/priority").and_then(priority_value_label))
}

fn priority_value_label(value: &Value) -> Option<String> {
    match value {
        Value::Number(number) => match number.as_i64()? {
            1 => Some("Urgent".to_owned()),
            2 => Some("High".to_owned()),
            3 => Some("Normal".to_owned()),
            4 => Some("Low".to_owned()),
            _ => None,
        },
        Value::String(raw) => Some(raw.trim().to_owned()).filter(|value| !value.is_empty()),
        _ => None,
    }
}

fn occurred_at(body: &Value, data: &Value) -> Option<DateTime<Utc>> {
    parse_datetime_at(body, &["/createdAt", "/updatedAt", "/webhookTimestamp"])
        .or_else(|| parse_datetime_at(data, &["/updatedAt", "/createdAt"]))
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

fn optional_text_at(body: &Value, pointers: &[&str]) -> Option<String> {
    pointers
        .iter()
        .find_map(|pointer| optional_text(body, pointer))
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
    fn parses_issue_event() {
        let payload = json!({
            "type": "Issue",
            "action": "create",
            "actor": { "name": "Ada" },
            "data": {
                "id": "issue-id",
                "identifier": "OPS-123",
                "title": "Fix ingestion",
                "priority": 1,
                "state": { "name": "Todo" },
                "assignee": { "name": "Grace" },
                "team": { "key": "OPS" },
                "createdAt": "2026-04-28T10:20:30Z"
            }
        });

        let parsed = match parse_linear_event(&payload) {
            Ok(parsed) => parsed,
            Err(error) => panic!("Linear issue event should parse: {error}"),
        };
        let expected = match Utc.with_ymd_and_hms(2026, 4, 28, 10, 20, 30).single() {
            Some(timestamp) => timestamp,
            None => panic!("test timestamp should be valid"),
        };

        assert_eq!(parsed.event_type, EventType::Issue);
        assert_eq!(parsed.actor, "Ada");
        assert_eq!(parsed.occurred_at, expected);
        assert_eq!(parsed.payload["priority"], "Urgent");
        assert_eq!(parsed.payload["identifier"], "OPS-123");
    }

    #[test]
    fn parses_comment_event() {
        let payload = json!({
            "type": "Comment",
            "action": "create",
            "actor": { "id": "user-id" },
            "data": {
                "id": "comment-id",
                "body": "Looks good",
                "issue": { "identifier": "OPS-123", "title": "Fix ingestion" },
                "createdAt": "2026-04-28T10:20:30Z"
            }
        });

        let parsed = match parse_linear_event(&payload) {
            Ok(parsed) => parsed,
            Err(error) => panic!("Linear comment event should parse: {error}"),
        };

        assert_eq!(parsed.event_type, EventType::IssueComment);
        assert_eq!(parsed.payload["body"], "Looks good");
        assert_eq!(parsed.payload["issue"]["identifier"], "OPS-123");
    }

    #[test]
    fn unknown_type_returns_validation_error() {
        let payload = json!({ "type": "Document", "action": "create" });

        assert!(matches!(
            parse_linear_event(&payload),
            Err(AppError::Validation(_))
        ));
    }

    #[test]
    fn idempotency_key_is_stable() {
        let timestamp = match Utc.with_ymd_and_hms(2026, 4, 28, 10, 20, 30).single() {
            Some(timestamp) => timestamp,
            None => panic!("test timestamp should be valid"),
        };

        assert_eq!(
            linear_idempotency_key("linear.issue.create", "OPS-123", timestamp),
            linear_idempotency_key("linear.issue.create", "OPS-123", timestamp)
        );
    }
}
