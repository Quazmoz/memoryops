pub mod handler;
pub mod parser;
pub mod validator;

#[cfg(test)]
mod tests {
    use common::models::EventType;
    use serde_json::json;

    use super::parser::parse_linear_event;

    #[test]
    fn parser_module_is_wired() {
        let payload = json!({
            "type": "Issue",
            "action": "create",
            "actor": { "name": "Ada" },
            "data": {
                "id": "issue-id",
                "identifier": "OPS-123",
                "title": "Fix ingestion",
                "priorityLabel": "High",
                "createdAt": "2026-04-28T10:20:30Z"
            }
        });

        let parsed = match parse_linear_event(&payload) {
            Ok(parsed) => parsed,
            Err(error) => panic!("Linear issue payload should parse: {error}"),
        };

        assert_eq!(parsed.event_type, EventType::Issue);
    }
}
