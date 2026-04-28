use axum::http::HeaderMap;
use common::AppError;
use hmac::{Hmac, Mac};
use sha2::Sha256;

pub(crate) const SIGNATURE_HEADER: &str = "x-linear-signature";

type HmacSha256 = Hmac<Sha256>;

pub fn verify_signature(headers: &HeaderMap, body: &[u8], secret: &str) -> Result<(), AppError> {
    let signature = required_header(headers, SIGNATURE_HEADER)?;
    let signature_bytes = hex::decode(signature).map_err(|_| AppError::Unauthorized)?;
    let mut mac =
        HmacSha256::new_from_slice(secret.as_bytes()).map_err(|_| AppError::Unauthorized)?;
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

#[cfg(test)]
mod tests {
    use axum::http::{HeaderMap, HeaderValue};

    use super::*;

    fn signed_header(body: &[u8], secret: &str) -> String {
        let mut mac = match HmacSha256::new_from_slice(secret.as_bytes()) {
            Ok(mac) => mac,
            Err(error) => panic!("HMAC should accept test secret: {error}"),
        };
        mac.update(body);
        hex::encode(mac.finalize().into_bytes())
    }

    fn headers(signature: String) -> HeaderMap {
        let mut headers = HeaderMap::new();
        let signature_value = match HeaderValue::from_str(&signature) {
            Ok(value) => value,
            Err(error) => panic!("signature header should build: {error}"),
        };
        headers.insert(SIGNATURE_HEADER, signature_value);
        headers
    }

    #[test]
    fn valid_hmac_passes() {
        let body = br#"{"type":"Issue","action":"create"}"#;
        let signature = signed_header(body, "secret");
        let headers = headers(signature);

        assert!(verify_signature(&headers, body, "secret").is_ok());
    }

    #[test]
    fn invalid_hmac_is_rejected() {
        let body = br#"{"type":"Issue","action":"create"}"#;
        let headers = headers("0000".to_owned());

        assert!(matches!(
            verify_signature(&headers, body, "secret"),
            Err(AppError::Unauthorized)
        ));
    }
}
