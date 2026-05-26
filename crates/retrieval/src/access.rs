use std::collections::HashMap;

use anyhow::anyhow;
use common::{error::AppResult, AppError};
use deadpool_redis::Pool as RedisPool;
use uuid::Uuid;

pub const ACCESS_KEY_PREFIX: &str = "memoryops:access:";
pub const ACCESS_TTL_SECS: u64 = 7_776_000;

pub async fn record_access(redis: &RedisPool, memory_id: Uuid) -> AppResult<u64> {
    let mut connection = match redis.get().await {
        Ok(conn) => conn,
        Err(error) => {
            tracing::warn!(error = ?error, memory_id = %memory_id, "failed to get Redis connection for access recording");
            return Ok(0);
        }
    };
    let key = access_key(memory_id);
    let result = redis::pipe()
        .cmd("HINCRBY")
        .arg(&key)
        .arg("count")
        .arg(1_i64)
        .cmd("EXPIRE")
        .arg(&key)
        .arg(ACCESS_TTL_SECS)
        .query_async::<(i64, bool)>(&mut *connection)
        .await;

    match result {
        Ok((count, _)) if count >= 0 => Ok(count as u64),
        Ok(_) => Ok(0),
        Err(error) => {
            tracing::warn!(error = ?error, memory_id = %memory_id, "failed to record access in Redis");
            Ok(0)
        }
    }
}

pub async fn record_access_batch(redis: &RedisPool, memory_ids: &[Uuid]) -> AppResult<()> {
    if memory_ids.is_empty() {
        return Ok(());
    }

    let mut connection = match redis.get().await {
        Ok(conn) => conn,
        Err(error) => {
            tracing::warn!(error = ?error, count = memory_ids.len(), "failed to get Redis connection for access batch recording");
            return Ok(());
        }
    };

    let mut pipe = redis::pipe();
    for memory_id in memory_ids {
        let key = access_key(*memory_id);
        pipe.cmd("HINCRBY")
            .arg(&key)
            .arg("count")
            .arg(1_i64)
            .ignore()
            .cmd("EXPIRE")
            .arg(&key)
            .arg(ACCESS_TTL_SECS)
            .ignore();
    }

    if let Err(error) = pipe.query_async::<()>(&mut *connection).await {
        tracing::warn!(error = ?error, count = memory_ids.len(), "failed to batch record memory access");
    }

    Ok(())
}

pub async fn get_access_counts(
    redis: &RedisPool,
    memory_ids: &[Uuid],
) -> AppResult<HashMap<Uuid, u64>> {
    if memory_ids.is_empty() {
        return Ok(HashMap::new());
    }

    let mut connection = match redis.get().await {
        Ok(conn) => conn,
        Err(error) => {
            tracing::warn!(error = ?error, count = memory_ids.len(), "failed to get Redis connection for access counts");
            return Ok(memory_ids.iter().map(|id| (*id, 0)).collect());
        }
    };

    let mut pipe = redis::pipe();
    for memory_id in memory_ids {
        pipe.cmd("HGET").arg(access_key(*memory_id)).arg("count");
    }

    let counts = pipe
        .query_async::<Vec<Option<u64>>>(&mut *connection)
        .await
        .map_err(|error| AppError::Internal(anyhow!(error)))?;

    Ok(memory_ids
        .iter()
        .copied()
        .zip(counts.into_iter().map(|count| count.unwrap_or(0)))
        .collect())
}

pub async fn get_access_count(redis: &RedisPool, memory_id: Uuid) -> AppResult<u64> {
    let mut connection = match redis.get().await {
        Ok(conn) => conn,
        Err(error) => {
            tracing::warn!(error = ?error, memory_id = %memory_id, "failed to get Redis connection for access count");
            return Ok(0);
        }
    };
    let key = access_key(memory_id);
    redis::cmd("HGET")
        .arg(&key)
        .arg("count")
        .query_async::<Option<u64>>(&mut *connection)
        .await
        .map(|count| count.unwrap_or(0))
        .map_err(|error| AppError::Internal(anyhow!(error)))
}

pub fn access_key(memory_id: Uuid) -> String {
    format!("{ACCESS_KEY_PREFIX}{memory_id}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn access_key_format_is_stable() {
        let memory_id = Uuid::from_u128(42);

        assert_eq!(
            access_key(memory_id),
            format!("{ACCESS_KEY_PREFIX}{memory_id}")
        );
    }

    #[tokio::test]
    #[ignore = "requires live Redis from docker-compose.test.yml"]
    async fn get_access_count_with_live_redis() {
        let redis = live_redis().await;
        let memory_id = Uuid::now_v7();

        let count = match get_access_count(&redis, memory_id).await {
            Ok(count) => count,
            Err(error) => panic!("access count lookup should succeed: {error}"),
        };

        assert_eq!(count, 0);
    }

    #[tokio::test]
    #[ignore = "requires live Redis from docker-compose.test.yml"]
    async fn record_access_with_live_redis() {
        let redis = live_redis().await;
        let memory_id = Uuid::now_v7();

        let count = match record_access(&redis, memory_id).await {
            Ok(count) => count,
            Err(error) => panic!("access record should succeed: {error}"),
        };

        assert_eq!(count, 1);
    }

    async fn live_redis() -> RedisPool {
        let redis_url =
            std::env::var("REDIS_URL").unwrap_or_else(|_| "redis://localhost:16379".to_owned());
        let cfg = deadpool_redis::Config::from_url(&redis_url);
        match cfg.create_pool(Some(deadpool_redis::Runtime::Tokio1)) {
            Ok(pool) => pool,
            Err(error) => panic!("test Redis pool should be created: {error}"),
        }
    }
}
