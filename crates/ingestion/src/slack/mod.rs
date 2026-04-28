pub mod handler;
pub mod parser;
pub mod validator;

#[cfg(test)]
mod tests {
    use common::models::EventType;
    use serde_json::json;

    use super::parser::parse_slack_event;

    #[test]
    fn parser_module_is_wired() {
        let payload = json!({
            "type": "event_callback",
            "event": {
                "type": "message",
                "channel": "C123",
                "user": "U123",
                "text": "ship it",
                "ts": "1712345678.123456"
            }
        });

        let parsed = match parse_slack_event(&payload) {
            Ok(parsed) => parsed,
            Err(error) => panic!("message payload should parse: {error}"),
        };

        assert_eq!(parsed.event_type, EventType::Message);
    }
}
