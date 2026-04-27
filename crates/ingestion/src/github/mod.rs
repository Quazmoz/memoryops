pub mod handler;
pub mod parser;
pub mod signature;

#[cfg(test)]
mod tests {
    use common::models::EventType;
    use serde_json::json;

    use super::parser::parse_github_event;

    #[test]
    fn parser_module_is_wired() {
        let payload = json!({
            "pusher": { "name": "mona" },
            "repository": { "pushed_at": 1_700_000_000_i64 }
        });

        let parsed = match parse_github_event("push", &payload) {
            Ok(parsed) => parsed,
            Err(error) => panic!("push payload should parse: {error}"),
        };

        assert_eq!(parsed.event_type, EventType::Push);
    }
}
