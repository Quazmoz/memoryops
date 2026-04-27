use std::time::Duration;

use anyhow::anyhow;
use common::{error::AppResult, AppError, AppState};
use ingestion::STREAM_KEY;
use redis::{
    aio::ConnectionManager, from_redis_value, streams::StreamId, streams::StreamReadReply, Value,
};
use tokio::task::JoinSet;
use uuid::Uuid;

use crate::{dlq, pipeline, store};

pub const GROUP_NAME: &str = "memoryops-processor";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedStreamMessage {
    pub stream_id: String,
    pub event_id: Uuid,
    pub workspace_id: Uuid,
}

pub async fn run_worker(worker_id: usize, state: AppState) {
    let consumer_name = format!("processor-{worker_id}");
    let mut redis = state.redis.clone();

    if let Err(error) = ensure_consumer_group(&mut redis).await {
        tracing::error!(error = ?error, "failed to ensure Redis consumer group");
    }

    loop {
        match read_new_messages(&mut redis, &consumer_name).await {
            Ok(messages) => {
                if messages.is_empty() {
                    continue;
                }

                let mut tasks = JoinSet::new();
                for message in messages {
                    let task_state = state.clone();
                    let mut task_redis = state.redis.clone();
                    tasks.spawn(async move {
                        if let Err(error) = process_stream_message(task_state, &mut task_redis, message).await {
                            tracing::error!(error = ?error, "failed to process Redis stream message");
                        }
                    });
                }

                while let Some(result) = tasks.join_next().await {
                    if let Err(error) = result {
                        tracing::error!(error = ?error, "processor message task panicked or was cancelled");
                    }
                }
            }
            Err(error) => {
                tracing::error!(error = ?error, "processor worker loop error");
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
        }
    }
}

async fn ensure_consumer_group(redis: &mut ConnectionManager) -> anyhow::Result<()> {
    let result = redis::cmd("XGROUP")
        .arg("CREATE")
        .arg(STREAM_KEY)
        .arg(GROUP_NAME)
        .arg("$")
        .arg("MKSTREAM")
        .query_async::<Value>(&mut *redis)
        .await;

    match result {
        Ok(_) => Ok(()),
        Err(error) if is_busy_group_error(&error) => Ok(()),
        Err(error) => Err(anyhow!(error)),
    }
}

async fn read_new_messages(
    redis: &mut ConnectionManager,
    consumer_name: &str,
) -> anyhow::Result<Vec<StreamId>> {
    let value = redis::cmd("XREADGROUP")
        .arg("GROUP")
        .arg(GROUP_NAME)
        .arg(consumer_name)
        .arg("COUNT")
        .arg(10)
        .arg("BLOCK")
        .arg(5000)
        .arg("STREAMS")
        .arg(STREAM_KEY)
        .arg(">")
        .query_async::<Value>(&mut *redis)
        .await?;

    parse_stream_read_reply(value)
}

fn parse_stream_read_reply(value: Value) -> anyhow::Result<Vec<StreamId>> {
    match value {
        Value::Nil => Ok(Vec::new()),
        other => {
            let reply: StreamReadReply = from_redis_value(&other)?;
            Ok(reply
                .keys
                .into_iter()
                .flat_map(|stream_key| stream_key.ids)
                .collect())
        }
    }
}

async fn process_stream_message(
    state: AppState,
    redis: &mut ConnectionManager,
    message: StreamId,
) -> AppResult<()> {
    let parsed = match parse_message_ids(&message) {
        Some(parsed) => parsed,
        None => {
            tracing::warn!(stream_id = %message.id, "skipping unparseable stream message");
            ack_message(redis, &message.id).await?;
            return Ok(());
        }
    };

    let raw_event = match store::get_raw_event(&state.db, parsed.event_id).await? {
        Some(raw_event) => raw_event,
        None => {
            tracing::warn!(event_id = %parsed.event_id, "raw event missing for stream message");
            ack_message(redis, &parsed.stream_id).await?;
            return Ok(());
        }
    };

    match store::insert_processing_state(&state.db, raw_event.id, raw_event.workspace_id).await? {
        store::ProcessingStateAction::Proceed => {}
        store::ProcessingStateAction::AlreadyDone => {
            tracing::debug!(event_id = %raw_event.id, "raw event already processed");
            ack_message(redis, &parsed.stream_id).await?;
            return Ok(());
        }
        store::ProcessingStateAction::AlreadyProcessing => {
            tracing::debug!(event_id = %raw_event.id, "raw event already being processed");
            ack_message(redis, &parsed.stream_id).await?;
            return Ok(());
        }
        store::ProcessingStateAction::AlreadyFailed => {
            tracing::debug!(event_id = %raw_event.id, "raw event previously failed");
            ack_message(redis, &parsed.stream_id).await?;
            return Ok(());
        }
    }

    match pipeline::process_event(&state, &raw_event).await {
        Ok(memory_unit) => {
            store::mark_processing_done(&state.db, raw_event.id).await?;
            ack_message(redis, &parsed.stream_id).await?;
            tracing::info!(
                event_id = %raw_event.id,
                memory_id = %memory_unit.id,
                "processed raw event into memory unit"
            );
        }
        Err(error) => {
            handle_processing_error(&state, redis, &raw_event, &parsed.stream_id, error).await?;
        }
    }

    Ok(())
}

async fn handle_processing_error(
    state: &AppState,
    redis: &mut ConnectionManager,
    raw_event: &common::models::RawEvent,
    stream_id: &str,
    error: AppError,
) -> AppResult<()> {
    let error_message = error.to_string();
    let attempts =
        store::increment_processing_attempts(&state.db, raw_event.id, &error_message).await?;
    let max_retries = i32::try_from(state.config.processor.max_retries).unwrap_or(i32::MAX);

    if attempts >= max_retries {
        store::mark_processing_failed(&state.db, raw_event.id, &error_message, attempts).await?;
        dlq::send_to_dlq(
            redis,
            raw_event,
            &error_message,
            state.config.processor.dlq_ttl_days,
        )
        .await?;
        ack_message(redis, stream_id).await?;
        tracing::error!(
            event_id = %raw_event.id,
            attempts,
            error = %error_message,
            "raw event exceeded processor retries and was sent to DLQ"
        );
    } else {
        tracing::warn!(
            event_id = %raw_event.id,
            attempts,
            error = %error_message,
            "raw event processing failed and will be retried"
        );
    }

    Ok(())
}

async fn ack_message(redis: &mut ConnectionManager, stream_id: &str) -> AppResult<()> {
    redis::cmd("XACK")
        .arg(STREAM_KEY)
        .arg(GROUP_NAME)
        .arg(stream_id)
        .query_async::<i64>(&mut *redis)
        .await
        .map(|_| ())
        .map_err(|error| AppError::Internal(anyhow!(error)))
}

pub fn parse_message_ids(message: &StreamId) -> Option<ParsedStreamMessage> {
    let event_id = message.get::<String>("event_id")?;
    let workspace_id = message.get::<String>("workspace_id")?;

    Some(ParsedStreamMessage {
        stream_id: message.id.clone(),
        event_id: Uuid::parse_str(&event_id).ok()?,
        workspace_id: Uuid::parse_str(&workspace_id).ok()?,
    })
}

fn is_busy_group_error(error: &redis::RedisError) -> bool {
    error.to_string().contains("BUSYGROUP")
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use redis::Value;

    use super::*;

    fn stream_message(event_id: &str, workspace_id: &str) -> StreamId {
        let mut map = HashMap::new();
        map.insert(
            "event_id".to_owned(),
            Value::BulkString(event_id.as_bytes().to_vec()),
        );
        map.insert(
            "workspace_id".to_owned(),
            Value::BulkString(workspace_id.as_bytes().to_vec()),
        );

        StreamId {
            id: "1700000000000-0".to_owned(),
            map,
        }
    }

    #[test]
    fn parses_message_ids_from_stream_fields() {
        let event_id = Uuid::now_v7();
        let workspace_id = Uuid::now_v7();
        let message = stream_message(&event_id.to_string(), &workspace_id.to_string());

        let parsed = match parse_message_ids(&message) {
            Some(parsed) => parsed,
            None => panic!("valid message should parse"),
        };

        assert_eq!(parsed.stream_id, "1700000000000-0");
        assert_eq!(parsed.event_id, event_id);
        assert_eq!(parsed.workspace_id, workspace_id);
    }

    #[test]
    fn unparseable_message_does_not_panic() {
        let workspace_id = Uuid::now_v7();
        let message = stream_message("not-a-uuid", &workspace_id.to_string());

        assert!(parse_message_ids(&message).is_none());
    }
}
