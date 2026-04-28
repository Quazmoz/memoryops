use axum::http::HeaderMap;
use chrono::Utc;
use common::AppError;
use hmac::{Hmac, Mac};
use sha2::Sha256;

const SIGNATURE_HEADER: &str = "x-slack-signature";
const TIMESTAMP_HEADER: &str = "x-slack-request-timestamp";
const SIGNATURE_VERSION: &str = "v0";
const MAX_TIMESTAMP_SKEW_SECS: i64 = 300;

type HmacSha256 = Hmac<Sha256>;

pub fn verify_signature(headers: &HeaderMap, body: &[u8], secret: &str) -> Result<(), AppError> {
    verify_signature_at(headers, body, secret, Utc::now().timestamp())
}

pub(crate) fn verify_signature_at(
    headers: &HeaderMap,
    body: &[u8],
    secret: &str,
    now_unix: i64,
) -> Result<(), AppError> {
    let signature = required_header(headers, SIGNATURE_HEADER)?;
    let timestamp = required_header(headers, TIMESTAMP_HEADER)?
        .parse::<i64>()
        .map_err(|_| AppError::Unauthorized)?;
    if is_stale_timestamp(timestamp, now_unix) {
        return Err(AppError::Unauthorized);
    }

    let signature_hex = signature
        .strip_prefix("v0=")
        .ok_or(AppError::Unauthorized)?;
    let signature_bytes = hex::decode(signature_hex).map_err(|_| AppError::Unauthorized)?;
    let mut mac =
        HmacSha256::new_from_slice(secret.as_bytes()).map_err(|_| AppError::Unauthorized)?;
    let base_prefix = format!("{SIGNATURE_VERSION}:{timestamp}:");
    mac.update(base_prefix.as_bytes());
    mac.update(body);
    mac.verify_slice(&signature_bytes)
        .map_err(|_| AppError::Unauthorized)
}

fn required_header<'a>(headers: &'a HeaderMap, name: &str) -> Result<&'a str, AppError> {
    headers
        .get(name)
        .ok_or(AppError::Unauthorized)?
        .to_str()
        .map_err(|_| AppError::Unauthorized)
}

fn is_stale_timestamp(timestamp: i64, now_unix: i64) -> bool {
    now_unix < timestamp.saturating_sub(MAX_TIMESTAMP_SKEW_SECS)
        || now_unix > timestamp.saturating_add(MAX_TIMESTAMP_SKEW_SECS)
}

#[cfg(test)]
mod tests {
    use axum::http::{HeaderMap, HeaderValue};

    use super::*;

    fn signed_header(body: &[u8], secret: &str, timestamp: i64) -> String {
        let mut mac = match HmacSha256::new_from_slice(secret.as_bytes()) {
            Ok(mac) => mac,
            Err(error) => panic!("HMAC should accept test secret: {error}"),
        };
        let base_prefix = format!("v0:{timestamp}:");
        mac.update(base_prefix.as_bytes());
        mac.update(body);
        format!("v0={}", hex::encode(mac.finalize().into_bytes()))
    }

    fn headers(signature: String, timestamp: i64) -> HeaderMap {
        let mut headers = HeaderMap::new();
        let signature_value = match HeaderValue::from_str(&signature) {
            Ok(value) => value,
            Err(error) => panic!("signature header should build: {error}"),
        };
        let timestamp_value = match HeaderValue::from_str(&timestamp.to_string()) {
            Ok(value) => value,
            Err(error) => panic!("timestamp header should build: {error}"),
        };
        headers.insert(SIGNATURE_HEADER, signature_value);
        headers.insert(TIMESTAMP_HEADER, timestamp_value);
        headers
    }

    #[test]
    fn valid_hmac_passes() {
        let body = br#"{"type":"event_callback"}"#;
        let timestamp = 1_712_345_678;
        let signature = signed_header(body, "secret", timestamp);
        let headers = headers(signature, timestamp);

        assert!(verify_signature_at(&headers, body, "secret", timestamp).is_ok());
    }

    #[test]
    fn invalid_hmac_is_rejected() {
        let body = br#"{"type":"event_callback"}"#;
        let timestamp = 1_712_345_678;
        let headers = headers("v0=0000".to_owned(), timestamp);

        assert!(matches!(
            verify_signature_at(&headers, body, "secret", timestamp),
            Err(AppError::Unauthorized)
        ));
    }

    #[test]
    fn stale_timestamp_is_rejected() {
        let body = br#"{"type":"event_callback"}"#;
        let timestamp = 1_712_345_678;
        let signature = signed_header(body, "secret", timestamp);
        let headers = headers(signature, timestamp);

        assert!(matches!(
            verify_signature_at(&headers, body, "secret", timestamp + 301),
            Err(AppError::Unauthorized)
        ));
    }
}
