use axum::http::{header::AUTHORIZATION, HeaderMap};
use serde_json::Value;

pub fn initialize_bearer_token(params: Option<&Value>) -> Option<String> {
    let raw = params?.get("_meta")?.get("auth")?.get("token")?.as_str()?;
    bearer_token(raw)
}

pub fn header_bearer_token(headers: &HeaderMap) -> Option<String> {
    let raw = headers.get(AUTHORIZATION)?.to_str().ok()?;
    bearer_token(raw)
}

pub fn bearer_token(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    let (scheme, token) = trimmed.split_once(' ')?;
    if scheme.eq_ignore_ascii_case("bearer") && !token.trim().is_empty() {
        Some(token.trim().to_owned())
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn extracts_initialize_bearer_token() {
        let params = json!({
            "_meta": {
                "auth": { "token": "Bearer mops_01234567_abc" }
            }
        });

        assert_eq!(
            initialize_bearer_token(Some(&params)),
            Some("mops_01234567_abc".to_owned())
        );
    }
}
