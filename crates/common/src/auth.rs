use std::sync::{Arc, OnceLock};

use anyhow::anyhow;
use argon2::{
    password_hash::{rand_core::OsRng, PasswordHash, SaltString},
    Argon2, PasswordHasher, PasswordVerifier,
};
use rand::TryRngCore;
use redis::aio::ConnectionLike;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use tokio::sync::Semaphore;
use uuid::Uuid;

use crate::{error::AppResult, models::ApiKey, AppError};

const API_KEY_PREFIX: &str = "mops";
const WORKSPACE_PREFIX_LEN: usize = 8;
const LEGACY_PREFIX_LEN_V1: usize = 8;
const LEGACY_PREFIX_LEN_V2: usize = 13;
const GENERATED_PREFIX_LEN: usize = 21;
const SUPPORTED_PREFIX_LENS: [usize; 3] = [
    GENERATED_PREFIX_LEN,
    LEGACY_PREFIX_LEN_V2,
    LEGACY_PREFIX_LEN_V1,
];
const RANDOM_BYTES_LEN: usize = 32;
const AUTH_CACHE_TTL_SECS: u64 = 30;
const LAST_USED_UPDATE_MAX_IN_FLIGHT: usize = 64;
static LAST_USED_UPDATE_PERMITS: OnceLock<Arc<Semaphore>> = OnceLock::new();

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthContext {
    pub workspace_id: Uuid,
    pub key_id: Uuid,
    pub key_prefix: String,
}

impl AuthContext {
    pub fn actor(&self) -> String {
        format!("api_key:{}", self.key_id)
    }
}

pub fn generate_api_key(workspace_id: Uuid) -> AppResult<(String, String)> {
    let workspace_simple = workspace_id.simple().to_string();
    let workspace_prefix = &workspace_simple[..WORKSPACE_PREFIX_LEN];
    let mut random_bytes = [0_u8; RANDOM_BYTES_LEN];
    rand::rngs::OsRng
        .try_fill_bytes(&mut random_bytes)
        .map_err(|error| {
            AppError::Internal(anyhow!("OS random number generator failed: {error}"))
        })?;
    let random_part = bs58::encode(random_bytes).into_string();
    let plaintext = format!("{API_KEY_PREFIX}_{workspace_prefix}_{random_part}");
    let prefix = plaintext[..GENERATED_PREFIX_LEN].to_owned();

    Ok((plaintext, prefix))
}

pub fn hash_secret(secret: &str) -> AppResult<String> {
    let salt = SaltString::generate(&mut OsRng);
    let params = if cfg!(debug_assertions) {
        argon2::Params::new(1024, 1, 1, None).map_err(|error| AppError::Internal(anyhow!(error)))?
    } else {
        argon2::Params::default()
    };
    let argon2 = Argon2::new(argon2::Algorithm::Argon2id, argon2::Version::V0x13, params);
    argon2
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
    api_key_prefixes(secret).and_then(|prefixes| prefixes.into_iter().next())
}

pub async fn validate_api_key(db: &PgPool, api_key: &str) -> AppResult<AuthContext> {
    validate_api_key_uncached(db, api_key).await
}

pub async fn validate_api_key_cached(
    db: &PgPool,
    redis: &mut impl ConnectionLike,
    api_key: &str,
) -> AppResult<AuthContext> {
    let cache_key = api_key_cache_key(api_key);
    if let Some(context) = read_auth_context_cache(redis, &cache_key).await {
        return Ok(context);
    }

    let context = validate_api_key_uncached(db, api_key).await?;
    write_auth_context_cache(redis, &cache_key, &context).await;
    Ok(context)
}

pub async fn invalidate_api_key_cache(
    redis: &mut impl ConnectionLike,
    key_id: Uuid,
) -> AppResult<()> {
    let index_key = auth_cache_index_key(key_id);
    let cache_key = redis::cmd("GET")
        .arg(&index_key)
        .query_async::<Option<String>>(&mut *redis)
        .await
        .map_err(|error| AppError::Internal(anyhow!(error)))?;

    let mut pipe = redis::pipe();
    pipe.cmd("DEL").arg(&index_key);
    if let Some(cache_key) = cache_key {
        pipe.cmd("DEL").arg(cache_key);
    }
    pipe.query_async::<i64>(&mut *redis)
        .await
        .map(|_| ())
        .map_err(|error| AppError::Internal(anyhow!(error)))
}

async fn validate_api_key_uncached(db: &PgPool, api_key: &str) -> AppResult<AuthContext> {
    let prefixes = api_key_prefixes(api_key).ok_or(AppError::Unauthorized)?;
    let candidates = find_candidate_keys(db, &prefixes).await?;

    for candidate in candidates {
        let secret = api_key.to_owned();
        let hash = candidate.key_hash.clone();

        let is_valid = tokio::task::spawn_blocking(move || verify_secret(&secret, &hash))
            .await
            .map_err(|error| AppError::Internal(anyhow!(error)))?;

        if is_valid {
            return Ok(AuthContext {
                workspace_id: candidate.workspace_id,
                key_id: candidate.id,
                key_prefix: candidate.prefix,
            });
        }
    }

    Err(AppError::Unauthorized)
}

async fn read_auth_context_cache(
    redis: &mut impl ConnectionLike,
    cache_key: &str,
) -> Option<AuthContext> {
    let cached = match tokio::time::timeout(
        std::time::Duration::from_millis(2000),
        redis::cmd("GET")
            .arg(cache_key)
            .query_async::<Option<String>>(&mut *redis),
    )
    .await
    {
        Ok(Ok(value)) => value,
        Ok(Err(error)) => {
            tracing::warn!(error = ?error, "failed to read API key auth cache");
            return None;
        }
        Err(_) => {
            tracing::warn!("timed out reading API key auth cache");
            return None;
        }
    };

    cached.and_then(|json| match serde_json::from_str::<AuthContext>(&json) {
        Ok(context) => Some(context),
        Err(error) => {
            tracing::warn!(error = ?error, "failed to decode API key auth cache payload");
            None
        }
    })
}

async fn write_auth_context_cache(
    redis: &mut impl ConnectionLike,
    cache_key: &str,
    context: &AuthContext,
) {
    let payload = match serde_json::to_string(context) {
        Ok(payload) => payload,
        Err(error) => {
            tracing::warn!(error = ?error, "failed to encode API key auth cache payload");
            return;
        }
    };

    let index_key = auth_cache_index_key(context.key_id);
    let result = tokio::time::timeout(
        std::time::Duration::from_millis(2000),
        redis::pipe()
            .cmd("SETEX")
            .arg(cache_key)
            .arg(AUTH_CACHE_TTL_SECS)
            .arg(payload)
            .cmd("SETEX")
            .arg(index_key)
            .arg(AUTH_CACHE_TTL_SECS)
            .arg(cache_key)
            .query_async::<()>(&mut *redis),
    )
    .await;

    match result {
        Ok(Err(error)) => tracing::warn!(error = ?error, "failed to write API key auth cache"),
        Err(_) => tracing::warn!("timed out writing API key auth cache"),
        Ok(Ok(_)) => {}
    }
}

fn api_key_cache_key(api_key: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(api_key.as_bytes());
    let digest = hasher.finalize();
    format!("auth:api_key:{}", hex::encode(digest))
}

fn auth_cache_index_key(key_id: Uuid) -> String {
    format!("auth:api_key:key_id:{key_id}")
}

pub fn spawn_last_used_update(db: PgPool, key_id: Uuid) {
    let permits = last_used_update_permits();
    let Ok(permit) = permits.try_acquire_owned() else {
        tracing::warn!(key_id = %key_id, "API key last-used update queue is full; dropping update");
        return;
    };

    tokio::spawn(async move {
        let _permit = permit;
        if let Err(error) = sqlx::query("UPDATE api_keys SET last_used_at = now() WHERE id = $1")
            .bind(key_id)
            .execute(&db)
            .await
        {
            tracing::warn!(error = ?error, key_id = %key_id, "failed to update API key last_used_at");
        }
    });
}

fn last_used_update_permits() -> Arc<Semaphore> {
    LAST_USED_UPDATE_PERMITS
        .get_or_init(|| Arc::new(Semaphore::new(LAST_USED_UPDATE_MAX_IN_FLIGHT)))
        .clone()
}

async fn find_candidate_keys(db: &PgPool, prefixes: &[String]) -> AppResult<Vec<ApiKey>> {
    sqlx::query_as::<_, ApiKey>(
        r#"
        SELECT id, workspace_id, name, key_hash, prefix, created_at, last_used_at, revoked, revoked_at
        FROM api_keys
        WHERE prefix = ANY($1)
          AND revoked = false
        ORDER BY char_length(prefix) DESC, created_at DESC
        "#,
    )
    .bind(prefixes)
    .fetch_all(db)
    .await
    .map_err(AppError::Database)
}

fn api_key_prefixes(secret: &str) -> Option<Vec<String>> {
    if !is_valid_api_key_format(secret) {
        return None;
    }

    Some(
        SUPPORTED_PREFIX_LENS
            .into_iter()
            .filter(|prefix_len| secret.len() > *prefix_len)
            .map(|prefix_len| secret[..prefix_len].to_owned())
            .collect(),
    )
}

fn is_valid_api_key_format(secret: &str) -> bool {
    if !secret.is_ascii() {
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
        let (key, prefix) = match generate_api_key(workspace_id) {
            Ok(generated) => generated,
            Err(error) => panic!("key should be generated: {error}"),
        };

        assert!(api_key_prefix(&key).is_some());
        assert_eq!(GENERATED_PREFIX_LEN, 21);
        assert_eq!(prefix.len(), GENERATED_PREFIX_LEN);
        assert_eq!(api_key_prefix(&key), Some(prefix.clone()));
        assert_eq!(
            api_key_prefixes(&key),
            Some(vec![
                prefix,
                key[..LEGACY_PREFIX_LEN_V2].to_owned(),
                key[..LEGACY_PREFIX_LEN_V1].to_owned(),
            ])
        );
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

    #[test]
    fn legacy_key_prefixes_are_still_supported() {
        let legacy_key = "mops_01234567_abcdef";

        assert_eq!(
            api_key_prefixes(legacy_key),
            Some(vec![
                legacy_key[..LEGACY_PREFIX_LEN_V2].to_owned(),
                legacy_key[..LEGACY_PREFIX_LEN_V1].to_owned(),
            ])
        );
    }
}
