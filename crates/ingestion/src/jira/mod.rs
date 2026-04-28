pub mod handler;
pub mod parser;
pub mod validator;

#[cfg(test)]
mod tests {
    use common::models::EventType;
    use serde_json::json;

    use super::parser::parse_jira_event;

    #[test]
    fn parser_module_is_wired() {
        let payload = json!({
            "webhookEvent": "jira:issue_created",
            "timestamp": 1_777_377_630_000_i64,
            "user": { "displayName": "Ada" },
            "issue": {
                "id": "10001",
                "key": "OPS-123",
                "fields": {
                    "summary": "Fix ingestion",
                    "priority": { "name": "High" }
                }
            }
        });

        let parsed = match parse_jira_event(&payload) {
            Ok(parsed) => parsed,
            Err(error) => panic!("Jira issue payload should parse: {error}"),
        };

        assert_eq!(parsed.event_type, EventType::Issue);
    }
}
