use chrono::{DateTime, Utc};
use common::{models::EventType, AppError};
use serde_json::Value;

#[derive(Debug, Clone)]
pub struct ParsedEvent {
    pub event_type: EventType,
    pub actor: String,
    pub occurred_at: DateTime<Utc>,
    pub payload: Value,
}

pub fn parse_github_event(event_header: &str, body: &Value) -> Result<ParsedEvent, AppError> {
    let event_type = match event_header {
        "pull_request" => EventType::PullRequest,
        "pull_request_review" => EventType::PullRequestReview,
        "push" => EventType::Push,
        "issue_comment" => EventType::IssueComment,
        "issues" => EventType::Issue,
        other => {
            return Err(AppError::Validation(format!(
                "unrecognized GitHub event: {other}"
            )))
        }
    };

    let actor = match event_header {
        "push" => required_string(body, "/pusher/name")?,
        _ => required_string(body, "/sender/login")?,
    };

    let occurred_at = occurred_at(event_header, body).unwrap_or_else(Utc::now);

    Ok(ParsedEvent {
        event_type,
        actor,
        occurred_at,
        payload: body.clone(),
    })
}

fn required_string(body: &Value, pointer: &str) -> Result<String, AppError> {
    body.pointer(pointer)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .ok_or_else(|| AppError::Validation("missing actor field".to_owned()))
}

fn occurred_at(event_header: &str, body: &Value) -> Option<DateTime<Utc>> {
    match event_header {
        "pull_request" => parse_rfc3339(body.pointer("/pull_request/updated_at")),
        "pull_request_review" => parse_rfc3339(body.pointer("/review/submitted_at")),
        "push" => parse_unix_timestamp(body.pointer("/repository/pushed_at")),
        "issue_comment" => parse_rfc3339(body.pointer("/comment/updated_at")),
        "issues" => parse_rfc3339(body.pointer("/issue/updated_at")),
        _ => None,
    }
}

fn parse_rfc3339(value: Option<&Value>) -> Option<DateTime<Utc>> {
    value
        .and_then(Value::as_str)
        .and_then(|raw| DateTime::parse_from_rfc3339(raw).ok())
        .map(|timestamp| timestamp.with_timezone(&Utc))
}

fn parse_unix_timestamp(value: Option<&Value>) -> Option<DateTime<Utc>> {
    value
        .and_then(|raw| match raw {
            Value::Number(number) => number.as_i64(),
            Value::String(raw) => raw.parse::<i64>().ok(),
            _ => None,
        })
        .and_then(|seconds| DateTime::<Utc>::from_timestamp(seconds, 0))
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;
    use serde_json::json;

    use super::*;

    fn utc_datetime(raw: &str) -> DateTime<Utc> {
        match DateTime::parse_from_rfc3339(raw) {
            Ok(timestamp) => timestamp.with_timezone(&Utc),
            Err(error) => panic!("test timestamp should parse: {error}"),
        }
    }

    #[test]
    fn parse_pull_request_event() {
        let payload = json!({
            "sender": { "login": "octocat" },
            "pull_request": { "updated_at": "2026-04-27T10:20:30Z" }
        });

        let parsed = match parse_github_event("pull_request", &payload) {
            Ok(parsed) => parsed,
            Err(error) => panic!("pull_request payload should parse: {error}"),
        };

        assert_eq!(parsed.event_type, EventType::PullRequest);
        assert_eq!(parsed.actor, "octocat");
        assert_eq!(parsed.occurred_at, utc_datetime("2026-04-27T10:20:30Z"));
        assert_eq!(parsed.payload, payload);
    }

    #[test]
    fn parse_push_event() {
        let payload = json!({
            "pusher": { "name": "mona" },
            "repository": { "pushed_at": 1_700_000_000_i64 }
        });

        let parsed = match parse_github_event("push", &payload) {
            Ok(parsed) => parsed,
            Err(error) => panic!("push payload should parse: {error}"),
        };
        let expected = match Utc.timestamp_opt(1_700_000_000, 0).single() {
            Some(timestamp) => timestamp,
            None => panic!("test unix timestamp should be valid"),
        };

        assert_eq!(parsed.event_type, EventType::Push);
        assert_eq!(parsed.actor, "mona");
        assert_eq!(parsed.occurred_at, expected);
    }

    #[test]
    fn unknown_event_returns_validation_error() {
        let payload = json!({ "sender": { "login": "octocat" } });

        assert!(matches!(
            parse_github_event("deployment", &payload),
            Err(AppError::Validation(_))
        ));
    }
}
