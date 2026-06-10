use anyhow::anyhow;
use common::{
    error::AppResult,
    models::{MemoryType, MemoryUnit, RawEvent},
    AppError,
};
use sqlx::PgPool;
use uuid::Uuid;

const MEMORY_COLUMNS: &str = "id, workspace_id, scope, memory_type, scope_visibility, content, entities, importance_score, importance_overridden, source_events, embedding_id, token_count, decay_score, relevance_score, pinned, tags, version, promoted_at, source_episode_ids, corroboration_count, deleted_at, last_accessed_at, created_at, updated_at";

#[derive(Debug, Clone)]
pub struct NewMemoryUnit {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub scope: serde_json::Value,
    pub memory_type: MemoryType,
    pub content: String,
    pub entities: serde_json::Value,
    pub importance_score: f32,
    pub source_events: Vec<Uuid>,
    pub embedding_id: Option<String>,
    pub token_count: Option<i32>,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessingStateAction {
    Proceed,
    ProceedStale,
    AlreadyDone,
    AlreadyProcessing,
    AlreadyFailed,
}

pub async fn insert_memory_unit(db: &PgPool, unit: &NewMemoryUnit) -> AppResult<MemoryUnit> {
    sqlx::query_as::<_, MemoryUnit>(
        r#"
        INSERT INTO memory_units (
            id,
            workspace_id,
            scope,
            memory_type,
            scope_visibility,
            content,
            entities,
            importance_score,
            source_events,
            embedding_id,
            token_count,
            tags
        )
        VALUES (
            $1,
            $2,
            $3,
            $4,
            CASE
                WHEN ($3::jsonb->>'agent_id') IS NULL AND ($3::jsonb->>'user_id') IS NULL THEN 'workspace'
                ELSE 'private'
            END,
            $5,
            $6,
            $7,
            $8,
            $9,
            $10,
            $11
        )
        RETURNING id,
            workspace_id,
            scope,
            memory_type,
            scope_visibility,
            content,
            entities,
            importance_score,
            importance_overridden,
            source_events,
            embedding_id,
            token_count,
            decay_score,
            relevance_score,
            pinned,
            tags,
            version,
            promoted_at,
            source_episode_ids,
            corroboration_count,
            deleted_at,
            last_accessed_at,
            created_at,
            updated_at
        "#,
    )
    .bind(unit.id)
    .bind(unit.workspace_id)
    .bind(&unit.scope)
    .bind(unit.memory_type)
    .bind(&unit.content)
    .bind(&unit.entities)
    .bind(unit.importance_score)
    .bind(&unit.source_events)
    .bind(&unit.embedding_id)
    .bind(unit.token_count)
    .bind(&unit.tags)
    .fetch_one(db)
    .await
    .map_err(AppError::Database)
}

pub async fn get_raw_event(db: &PgPool, id: Uuid) -> AppResult<Option<RawEvent>> {
    sqlx::query_as::<_, RawEvent>(
        r#"
        SELECT id,
            workspace_id,
            source,
            event_type,
            actor,
            payload,
            idempotency_key,
            occurred_at,
            ingested_at
        FROM raw_events
        WHERE id = $1
        "#,
    )
    .bind(id)
    .fetch_optional(db)
    .await
    .map_err(AppError::Database)
}

pub async fn get_memory_unit_by_id(
    db: &PgPool,
    id: Uuid,
    workspace_id: Uuid,
) -> AppResult<Option<MemoryUnit>> {
    let sql = format!(
        "SELECT {MEMORY_COLUMNS} FROM memory_units WHERE id = $1 AND workspace_id = $2 AND deleted_at IS NULL"
    );

    sqlx::query_as::<_, MemoryUnit>(&sql)
        .bind(id)
        .bind(workspace_id)
        .fetch_optional(db)
        .await
        .map_err(AppError::Database)
}

pub async fn update_memory_embedding(
    db: &PgPool,
    id: Uuid,
    workspace_id: Uuid,
    content: &str,
    embedding_id: &str,
    token_count: Option<i32>,
) -> AppResult<Option<MemoryUnit>> {
    let sql = format!(
        "UPDATE memory_units SET embedding_id = $3, content = $4, token_count = $5, updated_at = now() WHERE id = $1 AND workspace_id = $2 AND deleted_at IS NULL RETURNING {MEMORY_COLUMNS}"
    );

    sqlx::query_as::<_, MemoryUnit>(&sql)
        .bind(id)
        .bind(workspace_id)
        .bind(embedding_id)
        .bind(content)
        .bind(token_count)
        .fetch_optional(db)
        .await
        .map_err(AppError::Database)
}

pub async fn insert_processing_state(
    db: &PgPool,
    raw_event_id: Uuid,
    workspace_id: Uuid,
    stale_threshold_secs: i64,
) -> AppResult<ProcessingStateAction> {
    let inserted_status = sqlx::query_scalar::<_, String>(
        r#"
        INSERT INTO processing_state (raw_event_id, workspace_id, status)
        VALUES ($1, $2, 'processing')
        ON CONFLICT (raw_event_id) DO NOTHING
        RETURNING status
        "#,
    )
    .bind(raw_event_id)
    .bind(workspace_id)
    .fetch_optional(db)
    .await
    .map_err(AppError::Database)?;

    match inserted_status.as_deref() {
        Some("processing") => Ok(ProcessingStateAction::Proceed),
        Some(status) => Err(AppError::Internal(anyhow!(
            "unexpected inserted processing_state status: {status}"
        ))),
        None => existing_processing_state_action(db, raw_event_id, stale_threshold_secs).await,
    }
}

pub async fn mark_processing_done(db: &PgPool, raw_event_id: Uuid) -> AppResult<()> {
    sqlx::query(
        r#"
        UPDATE processing_state
        SET status = 'done', last_error = NULL, processed_at = now()
        WHERE raw_event_id = $1
        "#,
    )
    .bind(raw_event_id)
    .execute(db)
    .await
    .map(|_| ())
    .map_err(AppError::Database)
}

pub async fn mark_processing_failed(
    db: &PgPool,
    raw_event_id: Uuid,
    error: &str,
    attempts: i32,
) -> AppResult<()> {
    sqlx::query(
        r#"
        UPDATE processing_state
        SET status = 'failed', last_error = $2, attempts = $3, processed_at = now()
        WHERE raw_event_id = $1
        "#,
    )
    .bind(raw_event_id)
    .bind(error)
    .bind(attempts)
    .execute(db)
    .await
    .map(|_| ())
    .map_err(AppError::Database)
}

pub async fn increment_processing_attempts(
    db: &PgPool,
    raw_event_id: Uuid,
    error: &str,
) -> AppResult<i32> {
    sqlx::query_scalar::<_, i32>(
        r#"
        UPDATE processing_state
        SET attempts = attempts + 1, last_error = $2
        WHERE raw_event_id = $1
        RETURNING attempts
        "#,
    )
    .bind(raw_event_id)
    .bind(error)
    .fetch_one(db)
    .await
    .map_err(AppError::Database)
}

async fn existing_processing_state_action(
    db: &PgPool,
    raw_event_id: Uuid,
    stale_threshold_secs: i64,
) -> AppResult<ProcessingStateAction> {
    let stale_reclaimed = sqlx::query_scalar::<_, String>(
        r#"
        UPDATE processing_state
        SET status = 'processing',
            last_error = NULL,
            updated_at = now()
        WHERE raw_event_id = $1
          AND status = 'processing'
          AND updated_at < now() - ($2 * interval '1 second')
        RETURNING status
        "#,
    )
    .bind(raw_event_id)
    .bind(stale_threshold_secs)
    .fetch_optional(db)
    .await
    .map_err(AppError::Database)?;

    if stale_reclaimed.as_deref() == Some("processing") {
        return Ok(ProcessingStateAction::ProceedStale);
    }

    let existing_status = sqlx::query_scalar::<_, String>(
        "SELECT status FROM processing_state WHERE raw_event_id = $1",
    )
    .bind(raw_event_id)
    .fetch_one(db)
    .await
    .map_err(AppError::Database)?;

    match existing_status.as_str() {
        "done" => Ok(ProcessingStateAction::AlreadyDone),
        "processing" => Ok(ProcessingStateAction::AlreadyProcessing),
        "failed" => Ok(ProcessingStateAction::AlreadyFailed),
        status => Err(AppError::Internal(anyhow!(
            "unexpected processing_state status: {status}"
        ))),
    }
}
