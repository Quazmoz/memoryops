use chrono::{DateTime, Utc};
use common::{models::EventType, AppError};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParsedSlackEvent {
    pub event_type: EventType,
    pub event_kind: String,
    pub actor: String,
    pub channel_id: String,
    pub event_ts: String,
    pub thread_ts: Option<String>,
    pub occurred_at: DateTime<Utc>,
    pub payload: Value,
}

impl ParsedSlackEvent {
    pub fn idempotency_key(&self) -> String {
        slack_idempotency_key(&self.event_kind, &self.channel_id, &self.event_ts)
    }
}

pub fn parse_slack_event(body: &Value) -> Result<ParsedSlackEvent, AppError> {
    let event = body.get("event").unwrap_or(body);
    let event_type = required_string(event, "/type", "event.type")?;

    match event_type.as_str() {
        "message" => parse_message_event(event),
        "app_mention" => parse_plain_message_event(event, "app_mention"),
        "reaction_added" => parse_reaction_event(event),
        other => Err(AppError::Validation(format!(
            "unrecognized Slack event: {other}"
        ))),
    }
}

pub fn parse_slack_ts(ts: &str) -> Result<DateTime<Utc>, AppError> {
    let (seconds_raw, fraction_raw) = ts
        .split_once('.')
        .ok_or_else(|| AppError::Validation("invalid Slack timestamp".to_owned()))?;
    let seconds = seconds_raw
        .parse::<i64>()
        .map_err(|_| AppError::Validation("invalid Slack timestamp seconds".to_owned()))?;
    if fraction_raw.is_empty()
        || !fraction_raw
            .chars()
            .all(|character| character.is_ascii_digit())
    {
        return Err(AppError::Validation(
            "invalid Slack timestamp fraction".to_owned(),
        ));
    }

    let mut nanos_raw = fraction_raw.chars().take(9).collect::<String>();
    while nanos_raw.len() < 9 {
        nanos_raw.push('0');
    }
    let nanos = nanos_raw
        .parse::<u32>()
        .map_err(|_| AppError::Validation("invalid Slack timestamp nanos".to_owned()))?;

    DateTime::<Utc>::from_timestamp(seconds, nanos)
        .ok_or_else(|| AppError::Validation("Slack timestamp out of range".to_owned()))
}

pub fn slack_idempotency_key(event_kind: &str, channel_id: &str, ts: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"slack");
    hasher.update(event_kind.as_bytes());
    hasher.update(channel_id.as_bytes());
    hasher.update(ts.as_bytes());
    hex::encode(hasher.finalize())
}

fn parse_message_event(event: &Value) -> Result<ParsedSlackEvent, AppError> {
    let subtype = optional_string(event, "/subtype");
    if subtype.as_deref() == Some("message_changed") {
        parse_edited_message_event(event)
    } else {
        parse_plain_message_event(event, "message")
    }
}

fn parse_plain_message_event(
    event: &Value,
    event_kind: &str,
) -> Result<ParsedSlackEvent, AppError> {
    let channel_id = required_string(event, "/channel", "event.channel")?;
    let text = required_string(event, "/text", "event.text")?;
    let actor = required_string(event, "/user", "event.user")?;
    let event_ts = required_string(event, "/ts", "event.ts")?;
    let thread_ts = optional_string(event, "/thread_ts");
    let occurred_at = parse_slack_ts(&event_ts)?;
    let payload = json!({
        "type": event_kind,
        "channel": channel_id,
        "text": text,
        "user": actor,
        "ts": event_ts,
        "thread_ts": thread_ts,
    });

    Ok(ParsedSlackEvent {
        event_type: EventType::Message,
        event_kind: event_kind.to_owned(),
        actor,
        channel_id,
        event_ts,
        thread_ts,
        occurred_at,
        payload,
    })
}

fn parse_edited_message_event(event: &Value) -> Result<ParsedSlackEvent, AppError> {
    let message = event
        .get("message")
        .ok_or_else(|| AppError::Validation("missing event.message".to_owned()))?;
    let channel_id = required_string(event, "/channel", "event.channel")
        .or_else(|_| required_string(message, "/channel", "event.message.channel"))?;
    let text = required_string(message, "/text", "event.message.text")?;
    let actor = required_string(message, "/user", "event.message.user")?;
    let event_ts = required_string(message, "/ts", "event.message.ts")?;
    let thread_ts = optional_string(message, "/thread_ts");
    let occurred_at = parse_slack_ts(&event_ts)?;
    let payload = json!({
        "type": "message.edited",
        "channel": channel_id,
        "text": text,
        "user": actor,
        "ts": event_ts,
        "thread_ts": thread_ts,
    });

    Ok(ParsedSlackEvent {
        event_type: EventType::Message,
        event_kind: "message.edited".to_owned(),
        actor,
        channel_id,
        event_ts,
        thread_ts,
        occurred_at,
        payload,
    })
}

fn parse_reaction_event(event: &Value) -> Result<ParsedSlackEvent, AppError> {
    let actor = required_string(event, "/user", "event.user")?;
    let reaction = required_string(event, "/reaction", "event.reaction")?;
    let channel_id = required_string(event, "/item/channel", "event.item.channel")?;
    let event_ts = required_string(event, "/item/ts", "event.item.ts")?;
    let occurred_at = parse_slack_ts(&event_ts)?;
    let payload = json!({
        "type": "reaction_added",
        "user": actor,
        "reaction": reaction,
        "item": {
            "channel": channel_id,
            "ts": event_ts,
        },
        "channel": channel_id,
        "ts": event_ts,
    });

    Ok(ParsedSlackEvent {
        event_type: EventType::Reaction,
        event_kind: "reaction_added".to_owned(),
        actor,
        channel_id,
        event_ts,
        thread_ts: None,
        occurred_at,
        payload,
    })
}

fn required_string(body: &Value, pointer: &str, name: &str) -> Result<String, AppError> {
    body.pointer(pointer)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .ok_or_else(|| AppError::Validation(format!("missing {name}")))
}

fn optional_string(body: &Value, pointer: &str) -> Option<String> {
    body.pointer(pointer)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;
    use serde_json::json;

    use super::*;

    #[test]
    fn parses_message_event() {
        let payload = json!({
            "type": "event_callback",
            "event": {
                "type": "message",
                "channel": "C123",
                "user": "U123",
                "text": "hello <@U456> in #platform",
                "ts": "1712345678.123456"
            }
        });

        let parsed = match parse_slack_event(&payload) {
            Ok(parsed) => parsed,
            Err(error) => panic!("message event should parse: {error}"),
        };

        assert_eq!(parsed.event_type, EventType::Message);
        assert_eq!(parsed.event_kind, "message");
        assert_eq!(parsed.actor, "U123");
        assert_eq!(parsed.channel_id, "C123");
        assert!(parsed.thread_ts.is_none());
        assert_eq!(parsed.payload["text"], "hello <@U456> in #platform");
    }

    #[test]
    fn parses_message_edited_event() {
        let payload = json!({
            "type": "event_callback",
            "event": {
                "type": "message",
                "subtype": "message_changed",
                "channel": "C123",
                "message": {
                    "type": "message",
                    "user": "U123",
                    "text": "edited body",
                    "ts": "1712345678.123456",
                    "thread_ts": "1712345600.000001"
                }
            }
        });

        let parsed = match parse_slack_event(&payload) {
            Ok(parsed) => parsed,
            Err(error) => panic!("message.edited event should parse: {error}"),
        };

        assert_eq!(parsed.event_type, EventType::Message);
        assert_eq!(parsed.event_kind, "message.edited");
        assert_eq!(parsed.payload["text"], "edited body");
        assert_eq!(parsed.thread_ts, Some("1712345600.000001".to_owned()));
    }

    #[test]
    fn parses_reaction_added_event() {
        let payload = json!({
            "type": "event_callback",
            "event": {
                "type": "reaction_added",
                "user": "U123",
                "reaction": "eyes",
                "item": {
                    "type": "message",
                    "channel": "C123",
                    "ts": "1712345678.123456"
                }
            }
        });

        let parsed = match parse_slack_event(&payload) {
            Ok(parsed) => parsed,
            Err(error) => panic!("reaction_added event should parse: {error}"),
        };

        assert_eq!(parsed.event_type, EventType::Reaction);
        assert_eq!(parsed.event_kind, "reaction_added");
        assert_eq!(parsed.actor, "U123");
        assert_eq!(parsed.payload["reaction"], "eyes");
    }

    #[test]
    fn parses_app_mention_event() {
        let payload = json!({
            "type": "event_callback",
            "event": {
                "type": "app_mention",
                "channel": "C123",
                "user": "U123",
                "text": "<@UAPP> remember github.com/Quazmoz/memoryops",
                "ts": "1712345678.123456"
            }
        });

        let parsed = match parse_slack_event(&payload) {
            Ok(parsed) => parsed,
            Err(error) => panic!("app_mention event should parse: {error}"),
        };

        assert_eq!(parsed.event_type, EventType::Message);
        assert_eq!(parsed.event_kind, "app_mention");
        assert_eq!(
            parsed.payload["text"],
            "<@UAPP> remember github.com/Quazmoz/memoryops"
        );
    }

    #[test]
    fn parses_slack_timestamp() {
        let parsed = match parse_slack_ts("1712345678.123456") {
            Ok(timestamp) => timestamp,
            Err(error) => panic!("Slack timestamp should parse: {error}"),
        };
        let expected = match Utc.timestamp_opt(1_712_345_678, 123_456_000).single() {
            Some(timestamp) => timestamp,
            None => panic!("test timestamp should be valid"),
        };

        assert_eq!(parsed, expected);
    }

    #[test]
    fn unknown_event_type_returns_validation_error() {
        let payload = json!({
            "type": "event_callback",
            "event": { "type": "team_join" }
        });

        assert!(matches!(
            parse_slack_event(&payload),
            Err(AppError::Validation(_))
        ));
    }

    #[test]
    fn idempotency_key_uses_source_event_channel_and_ts() {
        let left = slack_idempotency_key("message", "C123", "1712345678.123456");
        let right = slack_idempotency_key("message", "C123", "1712345678.123456");
        let different = slack_idempotency_key("message", "C999", "1712345678.123456");

        assert_eq!(left, right);
        assert_ne!(left, different);
    }
}
