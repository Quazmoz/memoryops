use common::AppError;
use hmac::{Hmac, KeyInit, Mac};
use sha2::Sha256;

type HmacSha256 = Hmac<Sha256>;

pub fn verify_signature(header_value: &str, body: &[u8], secret: &str) -> Result<(), AppError> {
    let signature_hex = header_value
        .strip_prefix("sha256=")
        .ok_or(AppError::Unauthorized)?;
    let signature_bytes = hex::decode(signature_hex).map_err(|_| AppError::Unauthorized)?;

    let mut mac =
        HmacSha256::new_from_slice(secret.as_bytes()).map_err(|_| AppError::Unauthorized)?;
    mac.update(body);
    mac.verify_slice(&signature_bytes)
        .map_err(|_| AppError::Unauthorized)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn signed_header(body: &[u8], secret: &str) -> String {
        let mut mac = match HmacSha256::new_from_slice(secret.as_bytes()) {
            Ok(mac) => mac,
            Err(error) => panic!("HMAC should accept test secret: {error}"),
        };
        mac.update(body);
        let signature = hex::encode(mac.finalize().into_bytes());
        format!("sha256={signature}")
    }

    #[test]
    fn valid_signature_passes() {
        let body = br#"{"zen":"Keep it logically awesome."}"#;
        let header = signed_header(body, "test-secret");

        assert!(verify_signature(&header, body, "test-secret").is_ok());
    }

    #[test]
    fn invalid_signature_rejected() {
        let body = br#"{"zen":"Keep it logically awesome."}"#;
        let header = signed_header(body, "test-secret");
        let mut signature_bytes = match hex::decode(header.trim_start_matches("sha256=")) {
            Ok(bytes) => bytes,
            Err(error) => panic!("test signature should be valid hex: {error}"),
        };
        signature_bytes[0] ^= 0x01;
        let invalid_header = format!("sha256={}", hex::encode(signature_bytes));

        assert!(matches!(
            verify_signature(&invalid_header, body, "test-secret"),
            Err(AppError::Unauthorized)
        ));
    }

    #[test]
    fn missing_prefix_rejected() {
        assert!(matches!(
            verify_signature("abc123", b"{}", "test-secret"),
            Err(AppError::Unauthorized)
        ));
    }

    #[test]
    fn invalid_hex_rejected() {
        assert!(matches!(
            verify_signature("sha256=ZZZZ", b"{}", "test-secret"),
            Err(AppError::Unauthorized)
        ));
    }
}
