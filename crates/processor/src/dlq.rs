use chrono::Utc;
use common::{error::AppResult, models::RawEvent};
use serde_json::json;
use uuid::Uuid;

pub const DLQ_KEY_PREFIX: &str = "memoryops:dlq:";
pub const DLQ_LIST_PREFIX: &str = "dlq:";

pub async fn send_to_dlq(
    redis: &mut impl redis::aio::ConnectionLike,
    event: &RawEvent,
    error: &str,
    retry_count: i32,
    ttl_days: u64,
) -> AppResult<()> {
    let key = dlq_key(event.workspace_id, event.id);
    let list_key = dlq_list_key(event.workspace_id);
    let value = json!({
        "job_id": event.id,
        "event_id": event.id,
        "workspace_id": event.workspace_id,
        "payload": event.payload,
        "error": error,
        "retry_count": retry_count.max(0),
        "failed_at": Utc::now(),
    })
    .to_string();
    let ttl_seconds = ttl_days.saturating_mul(86_400);

    if let Err(redis_error) = redis::cmd("SETEX")
        .arg(&key)
        .arg(ttl_seconds)
        .arg(&value)
        .query_async::<redis::Value>(&mut *redis)
        .await
    {
        tracing::error!(error = ?redis_error, key = %key, "failed to write processor DLQ entry");
    }

    if let Err(redis_error) = redis::pipe()
        .cmd("LPUSH")
        .arg(&list_key)
        .arg(&value)
        .cmd("EXPIRE")
        .arg(&list_key)
        .arg(ttl_seconds)
        .query_async::<(i64, bool)>(&mut *redis)
        .await
    {
        tracing::error!(error = ?redis_error, key = %list_key, "failed to write processor DLQ list entry");
    }

    Ok(())
}

pub async fn send_processor_job_to_dlq(
    redis: &mut impl redis::aio::ConnectionLike,
    workspace_id: Uuid,
    memory_id: Uuid,
    error: &str,
    retry_count: i32,
    ttl_days: u64,
) -> AppResult<()> {
    let key = dlq_key(workspace_id, memory_id);
    let list_key = dlq_list_key(workspace_id);
    let value = json!({
        "job_id": memory_id,
        "memory_id": memory_id,
        "workspace_id": workspace_id,
        "payload": { "memory_id": memory_id },
        "error": error,
        "retry_count": retry_count.max(0),
        "failed_at": Utc::now(),
    })
    .to_string();
    let ttl_seconds = ttl_days.saturating_mul(86_400);

    if let Err(redis_error) = redis::cmd("SETEX")
        .arg(&key)
        .arg(ttl_seconds)
        .arg(&value)
        .query_async::<redis::Value>(&mut *redis)
        .await
    {
        tracing::error!(error = ?redis_error, key = %key, "failed to write slow processor DLQ entry");
    }

    if let Err(redis_error) = redis::pipe()
        .cmd("LPUSH")
        .arg(&list_key)
        .arg(&value)
        .cmd("EXPIRE")
        .arg(&list_key)
        .arg(ttl_seconds)
        .query_async::<(i64, bool)>(&mut *redis)
        .await
    {
        tracing::error!(error = ?redis_error, key = %list_key, "failed to write slow processor DLQ list entry");
    }

    Ok(())
}

pub fn dlq_key(workspace_id: Uuid, event_id: Uuid) -> String {
    format!("{DLQ_KEY_PREFIX}{workspace_id}:{event_id}")
}

pub fn dlq_list_key(workspace_id: Uuid) -> String {
    format!("{DLQ_LIST_PREFIX}{workspace_id}")
}

#[cfg(test)]
mod tests {
    use chrono::Utc;
    use common::models::{EventType, Source};
    use redis::aio::ConnectionManager;
    use serde_json::json;

    use super::*;

    fn raw_event() -> RawEvent {
        RawEvent {
            id: Uuid::now_v7(),
            workspace_id: Uuid::now_v7(),
            source: Source::GitHub,
            event_type: EventType::PullRequest,
            actor: "octocat".to_owned(),
            payload: json!({}),
            idempotency_key: "github:test".to_owned(),
            occurred_at: Utc::now(),
            ingested_at: Utc::now(),
        }
    }

    #[test]
    fn dlq_key_format_is_stable() {
        let workspace_id = Uuid::now_v7();
        let event_id = Uuid::now_v7();

        assert_eq!(
            dlq_key(workspace_id, event_id),
            format!("memoryops:dlq:{workspace_id}:{event_id}")
        );
        assert_eq!(dlq_list_key(workspace_id), format!("dlq:{workspace_id}"));
    }

    #[tokio::test]
    #[ignore = "requires live Redis from docker-compose.test.yml"]
    async fn send_to_dlq_with_live_redis() {
        let redis_url =
            std::env::var("REDIS_URL").unwrap_or_else(|_| "redis://localhost:16379".to_owned());
        let client = match redis::Client::open(redis_url) {
            Ok(client) => client,
            Err(error) => panic!("test Redis URL should be valid: {error}"),
        };
        let mut redis = match ConnectionManager::new(client).await {
            Ok(redis) => redis,
            Err(error) => panic!("test Redis should be reachable: {error}"),
        };
        let event = raw_event();

        if let Err(error) = send_to_dlq(&mut redis, &event, "boom", 3, 1).await {
            panic!("DLQ write should not fail caller: {error}");
        }
        let stored = match redis::cmd("GET")
            .arg(dlq_key(event.workspace_id, event.id))
            .query_async::<String>(&mut redis)
            .await
        {
            Ok(stored) => stored,
            Err(error) => panic!("DLQ entry should be readable: {error}"),
        };

        assert!(stored.contains("boom"));
    }
}
