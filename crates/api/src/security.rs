use aes_gcm::{
    aead::{rand_core::RngCore, Aead, OsRng},
    Aes256Gcm, KeyInit, Nonce,
};
use anyhow::anyhow;
use base64::{engine::general_purpose::STANDARD, Engine};
use common::AppError;
use hkdf::Hkdf;
use sha2::Sha256;
use zeroize::Zeroizing;

pub use common::auth::{generate_api_key, hash_secret};

const NONCE_LEN: usize = 12;
const SALT_LEN: usize = 32;
const HKDF_INFO: &[u8] = b"memoryops-skill-secret-v1";
const LEGACY_HKDF_SALT: &[u8] = b"memoryops-static-salt-v1";

pub fn encrypt_secret(plaintext: &str) -> Result<String, AppError> {
    let mut salt = [0u8; SALT_LEN];
    OsRng.fill_bytes(&mut salt);
    let cipher = cipher_from_key_and_salt(&salt)?;

    let mut nonce_bytes = [0_u8; NONCE_LEN];
    OsRng.fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);

    let ciphertext = cipher
        .encrypt(nonce, plaintext.as_bytes())
        .map_err(|error| AppError::Internal(anyhow!(error)))?;

    let mut payload = Vec::with_capacity(SALT_LEN + NONCE_LEN + ciphertext.len());
    payload.extend_from_slice(&salt);
    payload.extend_from_slice(&nonce_bytes);
    payload.extend_from_slice(&ciphertext);
    Ok(STANDARD.encode(payload))
}

pub fn decrypt_secret(ciphertext_b64: &str) -> Result<String, AppError> {
    let payload = STANDARD
        .decode(ciphertext_b64)
        .map_err(|error| AppError::Internal(anyhow!(error)))?;

    if payload.len() <= SALT_LEN + NONCE_LEN {
        return Err(AppError::Internal(anyhow!(
            "encrypted skill secret payload is invalid"
        )));
    }

    let (salt, rest) = payload.split_at(SALT_LEN);
    let (nonce_bytes, ciphertext) = rest.split_at(NONCE_LEN);

    let cipher = cipher_from_key_and_salt(salt)?;
    let plaintext = cipher
        .decrypt(Nonce::from_slice(nonce_bytes), ciphertext)
        .map_err(|error| AppError::Internal(anyhow!(error)))?;

    String::from_utf8(plaintext).map_err(|error| AppError::Internal(anyhow!(error)))
}

pub fn validate_secret_key_at_startup() -> Result<(), AppError> {
    let dummy_salt = [0u8; SALT_LEN];
    cipher_from_key_and_salt(&dummy_salt).map(|_| ())
}

/// Decrypt a secret that may be in either the legacy (static-salt) or
/// current (per-encryption random salt) format. Use this only during
/// a one-time re-encryption migration, then remove it.
pub fn decrypt_secret_legacy_or_current(ciphertext_b64: &str) -> Result<String, AppError> {
    let payload = STANDARD
        .decode(ciphertext_b64)
        .map_err(|error| AppError::Internal(anyhow!(error)))?;

    if payload.len() <= SALT_LEN + NONCE_LEN {
        return decrypt_secret_with_static_salt(&payload);
    }

    decrypt_secret(ciphertext_b64)
}

fn decrypt_secret_with_static_salt(payload: &[u8]) -> Result<String, AppError> {
    if payload.len() <= NONCE_LEN {
        return Err(AppError::Internal(anyhow!(
            "legacy encrypted skill secret payload is invalid"
        )));
    }

    let (nonce_bytes, ciphertext) = payload.split_at(NONCE_LEN);
    let cipher = cipher_from_key_and_salt(LEGACY_HKDF_SALT)?;
    let plaintext = cipher
        .decrypt(Nonce::from_slice(nonce_bytes), ciphertext)
        .map_err(|error| AppError::Internal(anyhow!(error)))?;

    String::from_utf8(plaintext).map_err(|error| AppError::Internal(anyhow!(error)))
}

fn cipher_from_key_and_salt(salt: &[u8]) -> Result<Aes256Gcm, AppError> {
    let secret = Zeroizing::new(std::env::var("APP_SECRET_KEY").map_err(|_| {
        AppError::Internal(anyhow!(
            "APP_SECRET_KEY must be set before storing or reading skill secrets"
        ))
    })?);
    let trimmed = secret.trim();
    if trimmed.is_empty() {
        return Err(AppError::Internal(anyhow!(
            "APP_SECRET_KEY must not be empty"
        )));
    }

    let hk = Hkdf::<Sha256>::new(Some(salt), trimmed.as_bytes());
    let mut key_bytes = Zeroizing::new([0u8; 32]);
    hk.expand(HKDF_INFO, key_bytes.as_mut())
        .map_err(|e| AppError::Internal(anyhow!("HKDF expand failed: {e}")))?;

    let key = aes_gcm::Key::<Aes256Gcm>::from_slice(key_bytes.as_ref());
    Ok(Aes256Gcm::new(key))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encrypt_decrypt_roundtrip() {
        std::env::set_var("APP_SECRET_KEY", "test-secret-key-for-unit-tests");
        let plaintext = "sk-ant-api-test-secret";
        let encrypted = match encrypt_secret(plaintext) {
            Ok(encrypted) => encrypted,
            Err(error) => panic!("encrypt should succeed: {error}"),
        };
        let decrypted = match decrypt_secret(&encrypted) {
            Ok(decrypted) => decrypted,
            Err(error) => panic!("decrypt should succeed: {error}"),
        };

        assert_eq!(plaintext, decrypted);
    }

    #[test]
    fn two_encryptions_produce_different_ciphertext() {
        std::env::set_var("APP_SECRET_KEY", "test-secret-key-for-unit-tests");
        let plaintext = "same-secret";
        let first = match encrypt_secret(plaintext) {
            Ok(encrypted) => encrypted,
            Err(error) => panic!("encrypt first should succeed: {error}"),
        };
        let second = match encrypt_secret(plaintext) {
            Ok(encrypted) => encrypted,
            Err(error) => panic!("encrypt second should succeed: {error}"),
        };

        assert_ne!(
            first, second,
            "each encryption should use a unique salt+nonce"
        );
    }
}
