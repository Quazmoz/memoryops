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
const HKDF_INFO: &[u8] = b"memoryops-skill-secret-v1";
const HKDF_SALT: &[u8] = b"memoryops-static-salt-v1";

pub fn encrypt_secret(plaintext: &str) -> Result<String, AppError> {
    let cipher = cipher_from_env()?;
    let mut nonce_bytes = [0_u8; NONCE_LEN];
    OsRng.fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);
    let ciphertext = cipher
        .encrypt(nonce, plaintext.as_bytes())
        .map_err(|error| AppError::Internal(anyhow!(error)))?;
    let mut payload = Vec::with_capacity(NONCE_LEN + ciphertext.len());
    payload.extend_from_slice(&nonce_bytes);
    payload.extend_from_slice(&ciphertext);
    Ok(STANDARD.encode(payload))
}

pub fn decrypt_secret(ciphertext_b64: &str) -> Result<String, AppError> {
    let payload = STANDARD
        .decode(ciphertext_b64)
        .map_err(|error| AppError::Internal(anyhow!(error)))?;
    if payload.len() <= NONCE_LEN {
        return Err(AppError::Internal(anyhow!(
            "encrypted skill secret payload is invalid"
        )));
    }

    let (nonce_bytes, ciphertext) = payload.split_at(NONCE_LEN);
    let cipher = cipher_from_env()?;
    let plaintext = cipher
        .decrypt(Nonce::from_slice(nonce_bytes), ciphertext)
        .map_err(|error| AppError::Internal(anyhow!(error)))?;

    String::from_utf8(plaintext).map_err(|error| AppError::Internal(anyhow!(error)))
}

pub fn validate_secret_key_at_startup() -> Result<(), AppError> {
    cipher_from_env().map(|_| ())
}

fn cipher_from_env() -> Result<Aes256Gcm, AppError> {
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

    let hk = Hkdf::<Sha256>::new(Some(HKDF_SALT), trimmed.as_bytes());
    let mut key_bytes = Zeroizing::new([0u8; 32]);
    hk.expand(HKDF_INFO, key_bytes.as_mut())
        .map_err(|e| AppError::Internal(anyhow!("HKDF expand failed: {e}")))?;

    let key = aes_gcm::Key::<Aes256Gcm>::from_slice(key_bytes.as_ref());
    Ok(Aes256Gcm::new(key))
}
