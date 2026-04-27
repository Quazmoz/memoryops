use anyhow::anyhow;
use argon2::{
    password_hash::{rand_core::OsRng, PasswordHash, SaltString},
    Argon2, PasswordHasher, PasswordVerifier,
};
use common::{error::AppResult, AppError};
use rand::RngCore;
use uuid::Uuid;

const API_KEY_PREFIX: &str = "mops";
const WORKSPACE_PREFIX_LEN: usize = 8;
const STORED_PREFIX_LEN: usize = 8;
const RANDOM_BYTES_LEN: usize = 32;

pub fn generate_api_key(workspace_id: Uuid) -> (String, String) {
    let workspace_simple = workspace_id.simple().to_string();
    let workspace_prefix = &workspace_simple[..WORKSPACE_PREFIX_LEN];
    let mut random_bytes = [0_u8; RANDOM_BYTES_LEN];
    rand::rngs::OsRng.fill_bytes(&mut random_bytes);
    let random_part = bs58::encode(random_bytes).into_string();
    let plaintext = format!("{API_KEY_PREFIX}_{workspace_prefix}_{random_part}");
    let prefix = plaintext[..STORED_PREFIX_LEN].to_owned();

    (plaintext, prefix)
}

pub fn hash_secret(secret: &str) -> AppResult<String> {
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default()
        .hash_password(secret.as_bytes(), &salt)
        .map(|hash| hash.to_string())
        .map_err(|error| AppError::Internal(anyhow!(error)))
}

pub fn verify_secret(secret: &str, hash: &str) -> bool {
    let Ok(parsed_hash) = PasswordHash::new(hash) else {
        return false;
    };

    Argon2::default()
        .verify_password(secret.as_bytes(), &parsed_hash)
        .is_ok()
}

pub fn api_key_prefix(secret: &str) -> Option<String> {
    if !is_valid_api_key_format(secret) {
        return None;
    }

    Some(secret[..STORED_PREFIX_LEN].to_owned())
}

fn is_valid_api_key_format(secret: &str) -> bool {
    if secret.len() <= STORED_PREFIX_LEN || !secret.is_ascii() {
        return false;
    }

    let mut parts = secret.split('_');
    let Some(prefix) = parts.next() else {
        return false;
    };
    let Some(workspace_prefix) = parts.next() else {
        return false;
    };
    let Some(random_part) = parts.next() else {
        return false;
    };

    prefix == API_KEY_PREFIX
        && workspace_prefix.len() == WORKSPACE_PREFIX_LEN
        && !random_part.is_empty()
        && parts.next().is_none()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_key_has_expected_format() {
        let workspace_id = Uuid::now_v7();
        let (key, prefix) = generate_api_key(workspace_id);

        assert!(api_key_prefix(&key).is_some());
        assert_eq!(prefix.len(), STORED_PREFIX_LEN);
        assert_eq!(api_key_prefix(&key), Some(prefix));
    }

    #[test]
    fn argon2_hash_verifies_original_secret() {
        let secret = "mops_01234567_abcdef";
        let hash = match hash_secret(secret) {
            Ok(hash) => hash,
            Err(error) => panic!("hash should be generated: {error}"),
        };

        assert!(verify_secret(secret, &hash));
        assert!(!verify_secret("mops_01234567_wrong", &hash));
    }
}
