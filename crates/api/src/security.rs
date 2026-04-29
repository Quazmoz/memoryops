use aes_gcm::{
    aead::{rand_core::RngCore, Aead, OsRng},
    Aes256Gcm, KeyInit, Nonce,
};
use anyhow::anyhow;
use base64::{engine::general_purpose::STANDARD, Engine};
use common::AppError;
use sha2::{Digest, Sha256};

pub use common::auth::{generate_api_key, hash_secret};

const NONCE_LEN: usize = 12;

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

fn cipher_from_env() -> Result<Aes256Gcm, AppError> {
    let secret = std::env::var("APP_SECRET_KEY").map_err(|_| {
        AppError::Internal(anyhow!(
            "APP_SECRET_KEY must be set before storing or reading skill secrets"
        ))
    })?;
    if secret.trim().is_empty() {
        return Err(AppError::Internal(anyhow!(
            "APP_SECRET_KEY must not be empty before storing or reading skill secrets"
        )));
    }

    let key = Sha256::digest(secret.as_bytes());
    Aes256Gcm::new_from_slice(&key).map_err(|error| AppError::Internal(anyhow!(error)))
}
