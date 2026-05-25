use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use anyhow::anyhow;
use async_trait::async_trait;
use common::{
    audit::spawn_audit_log,
    build_embedding_provider_for_workspace, build_llm_provider_for_workspace,
    error::AppResult,
    models::{AuditAction, MemoryUnit, WorkspaceConfig},
    providers::LlmProvider,
    telemetry::{LLM_LATENCY, SLOW_PATH_FAILED, SLOW_PATH_PROCESSED},
    AppError, AppState,
};
use ingestion::STREAM_KEY;
use redis::{
    aio::ConnectionLike, from_redis_value, streams::StreamId, streams::StreamReadReply, Value,
};
use sqlx::PgPool;
use tokio::task::JoinSet;
use tracing::Instrument;
use uuid::Uuid;

use crate::{
    contradiction, dlq,
    embedder::{Embedder, QdrantPayload},
    pipeline,
    pipeline::fast::count_tokens,
    store,
};

pub const GROUP_NAME: &str = "memoryops-processor";
pub const PROCESSOR_JOBS_STREAM: &str = "processor_jobs";
pub const SLOW_GROUP_NAME: &str = "slow_workers";

const SLOW_SUMMARY_MAX_TOKENS: usize = 256;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedStreamMessage {
    pub stream_id: String,
    pub event_id: Uuid,
    pub workspace_id: Uuid,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessorJob {
    pub stream_id: String,
    pub memory_id: Uuid,
    pub workspace_id: Uuid,
    pub attempts: i32,
}

#[async_trait]
trait SlowMemoryStore: Send + Sync {
    async fn get_memory_unit_by_id(
        &self,
        id: Uuid,
        workspace_id: Uuid,
    ) -> AppResult<Option<MemoryUnit>>;

    async fn update_memory_embedding(
        &self,
        id: Uuid,
        workspace_id: Uuid,
        content: &str,
        embedding_id: &str,
        token_count: Option<i32>,
    ) -> AppResult<Option<MemoryUnit>>;
}

struct PgSlowMemoryStore<'a> {
    db: &'a PgPool,
}

#[async_trait]
impl SlowMemoryStore for PgSlowMemoryStore<'_> {
    async fn get_memory_unit_by_id(
        &self,
        id: Uuid,
        workspace_id: Uuid,
    ) -> AppResult<Option<MemoryUnit>> {
        store::get_memory_unit_by_id(self.db, id, workspace_id).await
    }

    async fn update_memory_embedding(
        &self,
        id: Uuid,
        workspace_id: Uuid,
        content: &str,
        embedding_id: &str,
        token_count: Option<i32>,
    ) -> AppResult<Option<MemoryUnit>> {
        store::update_memory_embedding(
            self.db,
            id,
            workspace_id,
            content,
            embedding_id,
            token_count,
        )
        .await
    }
}

#[async_trait]
trait SlowPathEmbedder: Send + Sync {
    async fn embed_and_store(
        &self,
        memory_id: Uuid,
        workspace_id: Uuid,
        text: &str,
        payload: QdrantPayload,
    ) -> AppResult<String>;
}

struct QdrantSlowPathEmbedder {
    embedder: Embedder,
}

#[async_trait]
impl SlowPathEmbedder for QdrantSlowPathEmbedder {
    async fn embed_and_store(
        &self,
        memory_id: Uuid,
        workspace_id: Uuid,
        text: &str,
        payload: QdrantPayload,
    ) -> AppResult<String> {
        self.embedder
            .embed_and_store(memory_id, workspace_id, text, payload)
            .await
    }
}

pub async fn run_worker(worker_id: usize, state: AppState) {
    let consumer_name = format!("processor-{worker_id}");
    let mut redis = match state.redis.get().await {
        Ok(conn) => conn,
        Err(error) => {
            tracing::error!(error = ?error, "failed to acquire Redis connection for worker");
            return;
        }
    };

    if let Err(error) = ensure_consumer_group(&mut *redis, STREAM_KEY, GROUP_NAME).await {
        tracing::error!(error = ?error, "failed to ensure Redis consumer group");
    }

    loop {
        match read_reclaimed_or_new_messages(
            &state,
            &mut *redis,
            STREAM_KEY,
            GROUP_NAME,
            &consumer_name,
            5000,
        )
        .await
        {
            Ok(messages) => {
                if messages.is_empty() {
                    continue;
                }

                let mut tasks = JoinSet::new();
                for message in messages {
                    let task_state = state.clone();
                    let task_semaphore = Arc::clone(&state.processor_semaphore);
                    let span = tracing::info_span!("processor_task", stream_id = %message.id);
                    tasks.spawn(
                        async move {
                        let permit = task_semaphore.acquire_owned().await;
                        let Ok(_permit) = permit else {
                            tracing::error!("processor semaphore closed unexpectedly");
                            return;
                        };
                        let mut task_redis = match task_state.redis.get().await {
                            Ok(conn) => conn,
                            Err(error) => {
                                tracing::error!(error = ?error, "failed to acquire Redis connection for task");
                                return;
                            }
                        };
                        if let Err(error) =
                            process_stream_message(task_state, &mut *task_redis, message).await
                        {
                            tracing::error!(error = ?error, "failed to process Redis stream message");
                        }
                        }
                        .instrument(span),
                    );
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

pub async fn run_slow_worker(worker_id: usize, state: AppState) {
    let consumer_name = format!("slow-processor-{worker_id}");
    let mut redis = match state.redis.get().await {
        Ok(conn) => conn,
        Err(error) => {
            tracing::error!(error = ?error, "failed to acquire Redis connection for slow worker");
            return;
        }
    };

    if let Err(error) =
        ensure_consumer_group(&mut *redis, PROCESSOR_JOBS_STREAM, SLOW_GROUP_NAME).await
    {
        tracing::error!(error = ?error, "failed to ensure slow processor Redis consumer group");
    }

    loop {
        match read_reclaimed_or_new_messages(
            &state,
            &mut *redis,
            PROCESSOR_JOBS_STREAM,
            SLOW_GROUP_NAME,
            &consumer_name,
            2000,
        )
        .await
        {
            Ok(messages) => {
                if messages.is_empty() {
                    continue;
                }

                let mut tasks = JoinSet::new();
                for message in messages {
                    let task_state = state.clone();
                    let task_semaphore = Arc::clone(&state.processor_semaphore);
                    let span = tracing::info_span!("processor_task", stream_id = %message.id);
                    tasks.spawn(
                        async move {
                        let permit = task_semaphore.acquire_owned().await;
                        let Ok(_permit) = permit else {
                            tracing::error!("slow processor semaphore closed unexpectedly");
                            return;
                        };
                        let mut task_redis = match task_state.redis.get().await {
                            Ok(conn) => conn,
                            Err(error) => {
                                tracing::error!(error = ?error, "failed to acquire Redis connection for slow task");
                                return;
                            }
                        };
                        if let Err(error) =
                            process_slow_stream_message(task_state, &mut *task_redis, message).await
                        {
                            tracing::error!(error = ?error, "failed to process slow processor job");
                        }
                        }
                        .instrument(span),
                    );
                }

                while let Some(result) = tasks.join_next().await {
                    if let Err(error) = result {
                        tracing::error!(error = ?error, "slow processor task panicked or was cancelled");
                    }
                }
            }
            Err(error) => {
                tracing::error!(error = ?error, "slow processor worker loop error");
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
        }
    }
}

pub async fn enqueue_slow_job(
    redis: &mut impl ConnectionLike,
    memory_id: Uuid,
    workspace_id: Uuid,
    attempts: i32,
) -> AppResult<()> {
    redis::cmd("XADD")
        .arg(PROCESSOR_JOBS_STREAM)
        .arg("*")
        .arg("memory_id")
        .arg(memory_id.to_string())
        .arg("workspace_id")
        .arg(workspace_id.to_string())
        .arg("attempts")
        .arg(attempts.max(0).to_string())
        .query_async::<String>(&mut *redis)
        .await
        .map(|_| ())
        .map_err(|error| AppError::Internal(anyhow!(error)))
}

async fn ensure_consumer_group(
    redis: &mut impl ConnectionLike,
    stream_key: &str,
    group_name: &str,
) -> anyhow::Result<()> {
    let result = redis::cmd("XGROUP")
        .arg("CREATE")
        .arg(stream_key)
        .arg(group_name)
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
    redis: &mut impl ConnectionLike,
    stream_key: &str,
    group_name: &str,
    consumer_name: &str,
    block_ms: usize,
) -> anyhow::Result<Vec<StreamId>> {
    let value = redis::cmd("XREADGROUP")
        .arg("GROUP")
        .arg(group_name)
        .arg(consumer_name)
        .arg("COUNT")
        .arg(10)
        .arg("BLOCK")
        .arg(block_ms)
        .arg("STREAMS")
        .arg(stream_key)
        .arg(">")
        .query_async::<Value>(&mut *redis)
        .await?;

    parse_stream_read_reply(value)
}

async fn read_reclaimed_or_new_messages(
    state: &AppState,
    redis: &mut impl ConnectionLike,
    stream_key: &str,
    group_name: &str,
    consumer_name: &str,
    block_ms: usize,
) -> anyhow::Result<Vec<StreamId>> {
    let idle_ms = reclaim_idle_ms(state);
    match reclaim_pending_messages(redis, stream_key, group_name, consumer_name, idle_ms).await {
        Ok(messages) if !messages.is_empty() => {
            tracing::warn!(
                stream_key,
                group_name,
                consumer_name,
                count = messages.len(),
                "reclaimed stale Redis stream messages"
            );
            Ok(messages)
        }
        Ok(_) => read_new_messages(redis, stream_key, group_name, consumer_name, block_ms).await,
        Err(error) => {
            tracing::warn!(error = ?error, stream_key, group_name, "failed to reclaim stale Redis stream messages");
            read_new_messages(redis, stream_key, group_name, consumer_name, block_ms).await
        }
    }
}

async fn reclaim_pending_messages(
    redis: &mut impl ConnectionLike,
    stream_key: &str,
    group_name: &str,
    consumer_name: &str,
    min_idle_ms: usize,
) -> anyhow::Result<Vec<StreamId>> {
    let value = redis::cmd("XAUTOCLAIM")
        .arg(stream_key)
        .arg(group_name)
        .arg(consumer_name)
        .arg(min_idle_ms)
        .arg("0-0")
        .arg("COUNT")
        .arg(10)
        .query_async::<Value>(&mut *redis)
        .await?;

    parse_xautoclaim_reply(value)
}

fn reclaim_idle_ms(state: &AppState) -> usize {
    state
        .config
        .processor
        .processing_stale_threshold_secs
        .saturating_mul(1000)
        .min(usize::MAX as u64) as usize
}

fn parse_xautoclaim_reply(value: Value) -> anyhow::Result<Vec<StreamId>> {
    match value {
        Value::Nil => Ok(Vec::new()),
        Value::Array(values) if values.len() >= 2 => {
            from_redis_value(&values[1]).map_err(Into::into)
        }
        other => Err(anyhow!("unexpected XAUTOCLAIM reply: {other:?}")),
    }
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
    redis: &mut impl ConnectionLike,
    message: StreamId,
) -> AppResult<()> {
    let parsed = match parse_message_ids(&message) {
        Some(parsed) => parsed,
        None => {
            tracing::warn!(stream_id = %message.id, "skipping unparseable stream message");
            ack_message(redis, STREAM_KEY, GROUP_NAME, &message.id).await?;
            return Ok(());
        }
    };

    let raw_event = match store::get_raw_event(&state.db, parsed.event_id).await? {
        Some(raw_event) => raw_event,
        None => {
            tracing::warn!(event_id = %parsed.event_id, "raw event missing for stream message");
            ack_message(redis, STREAM_KEY, GROUP_NAME, &parsed.stream_id).await?;
            return Ok(());
        }
    };

    let stale_threshold_secs =
        i64::try_from(state.config.processor.processing_stale_threshold_secs).unwrap_or(600);
    match store::insert_processing_state(
        &state.db,
        raw_event.id,
        raw_event.workspace_id,
        stale_threshold_secs,
    )
    .await?
    {
        store::ProcessingStateAction::Proceed => {}
        store::ProcessingStateAction::ProceedStale => {
            tracing::warn!(event_id = %raw_event.id, "reclaimed stale processing state; retrying event");
        }
        store::ProcessingStateAction::AlreadyDone => {
            tracing::debug!(event_id = %raw_event.id, "raw event already processed");
            ack_message(redis, STREAM_KEY, GROUP_NAME, &parsed.stream_id).await?;
            return Ok(());
        }
        store::ProcessingStateAction::AlreadyProcessing => {
            tracing::debug!(event_id = %raw_event.id, "raw event already being processed");
            ack_message(redis, STREAM_KEY, GROUP_NAME, &parsed.stream_id).await?;
            return Ok(());
        }
        store::ProcessingStateAction::AlreadyFailed => {
            tracing::debug!(event_id = %raw_event.id, "raw event previously failed");
            ack_message(redis, STREAM_KEY, GROUP_NAME, &parsed.stream_id).await?;
            return Ok(());
        }
    }

    match pipeline::process_event(&state, &raw_event).await {
        Ok(memory_unit) => {
            if let Err(error) =
                enqueue_slow_job(redis, memory_unit.id, memory_unit.workspace_id, 0).await
            {
                tracing::error!(
                    error = ?error,
                    memory_id = %memory_unit.id,
                    "failed to enqueue slow processor job"
                );
            }
            store::mark_processing_done(&state.db, raw_event.id).await?;
            ack_message(redis, STREAM_KEY, GROUP_NAME, &parsed.stream_id).await?;
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

async fn process_slow_stream_message(
    state: AppState,
    redis: &mut impl ConnectionLike,
    message: StreamId,
) -> AppResult<()> {
    let job = match parse_processor_job(&message) {
        Some(job) => job,
        None => {
            tracing::warn!(stream_id = %message.id, "skipping unparseable slow processor job");
            if let Err(error) =
                ack_message(redis, PROCESSOR_JOBS_STREAM, SLOW_GROUP_NAME, &message.id).await
            {
                tracing::error!(error = ?error, stream_id = %message.id, "failed to ack bad slow processor job");
            }
            return Ok(());
        }
    };

    match process_slow(&state, job.clone()).await {
        Ok(()) => {
            SLOW_PATH_PROCESSED.add(1, &[]);
            if let Err(error) = ack_message(
                redis,
                PROCESSOR_JOBS_STREAM,
                SLOW_GROUP_NAME,
                &job.stream_id,
            )
            .await
            {
                tracing::error!(error = ?error, stream_id = %job.stream_id, "failed to ack slow processor job");
            }
        }
        Err(error) => handle_slow_processing_error(&state, redis, &job, error).await?,
    }

    Ok(())
}

pub async fn process_slow(state: &AppState, job: ProcessorJob) -> AppResult<()> {
    let workspace_config = fetch_workspace_config(&state.db, job.workspace_id).await?;
    let llm_provider = build_llm_provider_for_workspace(&state.config, &workspace_config);
    let embedding_provider = build_embedding_provider_for_workspace(&state.config, &workspace_config);
    let memory_store = PgSlowMemoryStore { db: &state.db };
    let embedder = QdrantSlowPathEmbedder {
        embedder: Embedder::new(embedding_provider, state.qdrant.clone()),
    };

    let updated = process_slow_with_dependencies(
        job,
        &memory_store,
        llm_provider.as_ref(),
        &embedder,
        Some(state.db.clone()),
    )
    .await?;

    if let Some(updated_memory) = updated {
        let task_state = state.clone();
        tokio::spawn(async move {
            let config = match contradiction::fetch_workspace_config(
                &task_state.db,
                updated_memory.workspace_id,
            )
            .await
            {
                Ok(config) => config,
                Err(error) => {
                    tracing::warn!(error = ?error, workspace_id = %updated_memory.workspace_id, "failed to load contradiction config");
                    return;
                }
            };

            if let Err(error) =
                contradiction::check_contradictions(&task_state, &updated_memory, &config).await
            {
                tracing::warn!(error = ?error, memory_id = %updated_memory.id, "contradiction check failed");
            }
        });
    }

    Ok(())
}

async fn fetch_workspace_config(db: &PgPool, workspace_id: Uuid) -> AppResult<WorkspaceConfig> {
    let value = sqlx::query_scalar::<_, serde_json::Value>(
        "SELECT config FROM workspaces WHERE id = $1 AND deleted_at IS NULL",
    )
    .bind(workspace_id)
    .fetch_optional(db)
    .await
    .map_err(AppError::Database)?
    .ok_or_else(|| AppError::NotFound {
        resource: format!("workspace:{workspace_id}"),
    })?;

    Ok(serde_json::from_value::<WorkspaceConfig>(value).unwrap_or_default())
}

async fn process_slow_with_dependencies(
    job: ProcessorJob,
    memory_store: &dyn SlowMemoryStore,
    llm_provider: &dyn LlmProvider,
    embedder: &dyn SlowPathEmbedder,
    audit_db: Option<PgPool>,
) -> AppResult<Option<MemoryUnit>> {
    let memory = match memory_store
        .get_memory_unit_by_id(job.memory_id, job.workspace_id)
        .await?
    {
        Some(memory) => memory,
        None => {
            tracing::debug!(memory_id = %job.memory_id, "slow processor memory missing or deleted; skipping");
            return Ok(None);
        }
    };

    if memory.embedding_id.is_some() {
        tracing::debug!(memory_id = %memory.id, "memory already has embedding_id; skipping slow path");
        return Ok(None);
    }

    let content = summarize_or_content(llm_provider, memory.id, &memory.content).await;
    let token_count = count_tokens(&content).ok();
    let payload = QdrantPayload::from_memory_unit(&memory);
    let embedding_id = embedder
        .embed_and_store(memory.id, memory.workspace_id, &content, payload)
        .await?;

    let updated = memory_store
        .update_memory_embedding(
            memory.id,
            memory.workspace_id,
            &content,
            &embedding_id,
            token_count,
        )
        .await?;

    if let (Some(updated_memory), Some(db)) = (&updated, audit_db) {
        spawn_audit_log(
            db,
            updated_memory.workspace_id,
            "system".to_owned(),
            AuditAction::MemoryEmbedded,
            updated_memory.id,
            "memory",
            Some(serde_json::json!({ "embedding_id": embedding_id })),
        );
    }

    Ok(updated)
}

async fn summarize_or_content(
    llm_provider: &dyn LlmProvider,
    memory_id: Uuid,
    content: &str,
) -> String {
    let started = Instant::now();
    let result = llm_provider
        .summarize(content, SLOW_SUMMARY_MAX_TOKENS)
        .await;
    LLM_LATENCY.record(elapsed_ms(started), &[]);

    match result {
        Ok(summary) if summary.trim().is_empty() => content.to_owned(),
        Ok(summary) => summary,
        Err(error) => {
            tracing::warn!(error = ?error, memory_id = %memory_id, "LLM summarization failed; using original memory content");
            content.to_owned()
        }
    }
}

fn elapsed_ms(started: Instant) -> f64 {
    started.elapsed().as_secs_f64() * 1_000.0
}

async fn handle_processing_error(
    state: &AppState,
    redis: &mut impl ConnectionLike,
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
            attempts,
            state.config.processor.dlq_ttl_days,
        )
        .await?;
        ack_message(redis, STREAM_KEY, GROUP_NAME, stream_id).await?;
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

async fn handle_slow_processing_error(
    state: &AppState,
    redis: &mut impl ConnectionLike,
    job: &ProcessorJob,
    error: AppError,
) -> AppResult<()> {
    let error_message = error.to_string();
    let attempts = job.attempts.saturating_add(1);
    let max_retries = i32::try_from(state.config.processor.max_retries).unwrap_or(i32::MAX);

    if attempts >= max_retries {
        SLOW_PATH_FAILED.add(1, &[]);
        dlq::send_processor_job_to_dlq(
            redis,
            job.workspace_id,
            job.memory_id,
            &error_message,
            attempts,
            state.config.processor.dlq_ttl_days,
        )
        .await?;
        if let Err(error) = ack_message(
            redis,
            PROCESSOR_JOBS_STREAM,
            SLOW_GROUP_NAME,
            &job.stream_id,
        )
        .await
        {
            tracing::error!(error = ?error, stream_id = %job.stream_id, "failed to ack DLQ slow processor job");
        }
        tracing::error!(
            memory_id = %job.memory_id,
            attempts,
            error = %error_message,
            "slow processor job exceeded retries and was sent to DLQ"
        );
    } else {
        enqueue_slow_job(redis, job.memory_id, job.workspace_id, attempts).await?;
        if let Err(error) = ack_message(
            redis,
            PROCESSOR_JOBS_STREAM,
            SLOW_GROUP_NAME,
            &job.stream_id,
        )
        .await
        {
            tracing::error!(error = ?error, stream_id = %job.stream_id, "failed to ack requeued slow processor job");
        }
        tracing::warn!(
            memory_id = %job.memory_id,
            attempts,
            error = %error_message,
            "slow processor job failed and was requeued"
        );
    }

    Ok(())
}

async fn ack_message(
    redis: &mut impl ConnectionLike,
    stream_key: &str,
    group_name: &str,
    stream_id: &str,
) -> AppResult<()> {
    redis::cmd("XACK")
        .arg(stream_key)
        .arg(group_name)
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

pub fn parse_processor_job(message: &StreamId) -> Option<ProcessorJob> {
    let memory_id = message.get::<String>("memory_id")?;
    let workspace_id = message.get::<String>("workspace_id")?;
    let attempts = message
        .get::<String>("attempts")
        .and_then(|value| value.parse::<i32>().ok())
        .unwrap_or_default();

    Some(ProcessorJob {
        stream_id: message.id.clone(),
        memory_id: Uuid::parse_str(&memory_id).ok()?,
        workspace_id: Uuid::parse_str(&workspace_id).ok()?,
        attempts,
    })
}

fn is_busy_group_error(error: &redis::RedisError) -> bool {
    error.code() == Some("BUSYGROUP")
}

#[cfg(test)]
mod tests {
    use std::{
        collections::HashMap,
        sync::atomic::{AtomicUsize, Ordering},
    };

    use common::{
        error::ProviderError,
        models::{MemoryScope, MemoryType},
    };
    use redis::Value;
    use sqlx::types::Json;

    use super::*;

    #[derive(Default)]
    struct MockSlowMemoryStore {
        memory: Option<MemoryUnit>,
        updates: AtomicUsize,
    }

    #[async_trait]
    impl SlowMemoryStore for MockSlowMemoryStore {
        async fn get_memory_unit_by_id(
            &self,
            _id: Uuid,
            _workspace_id: Uuid,
        ) -> AppResult<Option<MemoryUnit>> {
            Ok(self.memory.clone())
        }

        async fn update_memory_embedding(
            &self,
            _id: Uuid,
            _workspace_id: Uuid,
            _content: &str,
            _embedding_id: &str,
            _token_count: Option<i32>,
        ) -> AppResult<Option<MemoryUnit>> {
            self.updates.fetch_add(1, Ordering::SeqCst);
            Ok(None)
        }
    }

    #[derive(Default)]
    struct MockSlowPathEmbedder {
        calls: AtomicUsize,
    }

    #[async_trait]
    impl SlowPathEmbedder for MockSlowPathEmbedder {
        async fn embed_and_store(
            &self,
            memory_id: Uuid,
            _workspace_id: Uuid,
            _text: &str,
            _payload: QdrantPayload,
        ) -> AppResult<String> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            Ok(memory_id.to_string())
        }
    }

    #[derive(Default)]
    struct MockLlmProvider {
        calls: AtomicUsize,
    }

    #[async_trait]
    impl LlmProvider for MockLlmProvider {
        async fn complete(&self, _prompt: &str) -> Result<String, ProviderError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            Ok("complete".to_owned())
        }

        async fn summarize(&self, text: &str, _max_tokens: usize) -> Result<String, ProviderError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            Ok(text.to_owned())
        }
    }

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

    fn processor_job(memory_id: &str, workspace_id: &str, attempts: &str) -> StreamId {
        let mut map = HashMap::new();
        map.insert(
            "memory_id".to_owned(),
            Value::BulkString(memory_id.as_bytes().to_vec()),
        );
        map.insert(
            "workspace_id".to_owned(),
            Value::BulkString(workspace_id.as_bytes().to_vec()),
        );
        map.insert(
            "attempts".to_owned(),
            Value::BulkString(attempts.as_bytes().to_vec()),
        );
        StreamId {
            id: "1700000000000-1".to_owned(),
            map,
        }
    }

    fn memory_unit(id: Uuid, workspace_id: Uuid, embedding_id: Option<String>) -> MemoryUnit {
        let now = chrono::Utc::now();
        MemoryUnit {
            id,
            workspace_id,
            scope: MemoryScope {
                workspace_id,
                agent_id: None,
                user_id: None,
                repo: Some("Quazmoz/memoryops".to_owned()),
            },
            memory_type: MemoryType::Episodic,
            scope_visibility: common::models::ScopeVisibility::Private,
            content: "already embedded memory".to_owned(),
            entities: Json(Vec::new()),
            importance_score: 0.8,
            importance_overridden: false,
            source_events: Vec::new(),
            embedding_id,
            token_count: Some(3),
            decay_score: 1.0,
            relevance_score: 0.5,
            pinned: false,
            tags: Vec::new(),
            version: 1,
            promoted_at: None,
            source_episode_ids: Vec::new(),
            corroboration_count: 1,
            deleted_at: None,
            last_accessed_at: None,
            created_at: now,
            updated_at: now,
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

    #[test]
    fn parses_processor_job_from_stream_fields() {
        let memory_id = Uuid::now_v7();
        let workspace_id = Uuid::now_v7();
        let message = processor_job(&memory_id.to_string(), &workspace_id.to_string(), "2");

        let parsed = match parse_processor_job(&message) {
            Some(parsed) => parsed,
            None => panic!("valid processor job should parse"),
        };

        assert_eq!(parsed.stream_id, "1700000000000-1");
        assert_eq!(parsed.memory_id, memory_id);
        assert_eq!(parsed.workspace_id, workspace_id);
        assert_eq!(parsed.attempts, 2);
    }

    #[tokio::test]
    async fn process_slow_skips_memory_with_existing_embedding_id() {
        let memory_id = Uuid::now_v7();
        let workspace_id = Uuid::now_v7();
        let store = MockSlowMemoryStore {
            memory: Some(memory_unit(
                memory_id,
                workspace_id,
                Some("existing".to_owned()),
            )),
            updates: AtomicUsize::new(0),
        };
        let llm_provider = MockLlmProvider::default();
        let embedder = MockSlowPathEmbedder::default();
        let job = ProcessorJob {
            stream_id: "1700000000000-1".to_owned(),
            memory_id,
            workspace_id,
            attempts: 0,
        };

        if let Err(error) =
            process_slow_with_dependencies(job, &store, &llm_provider, &embedder, None).await
        {
            panic!("existing embedding should skip slow path: {error}");
        }

        assert_eq!(llm_provider.calls.load(Ordering::SeqCst), 0);
        assert_eq!(embedder.calls.load(Ordering::SeqCst), 0);
        assert_eq!(store.updates.load(Ordering::SeqCst), 0);
    }
}
