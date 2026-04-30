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
            content,
            entities,
            importance_score,
            source_events,
            embedding_id,
            token_count,
            tags
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
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

    if stale_reclaimed.is_some() {
        return Ok(ProcessingStateAction::ProceedStale);
    }

    let status = sqlx::query_scalar::<_, String>(
        "SELECT status FROM processing_state WHERE raw_event_id = $1",
    )
    .bind(raw_event_id)
    .fetch_optional(db)
    .await
    .map_err(AppError::Database)?;

    match status.as_deref() {
        Some("done") => Ok(ProcessingStateAction::AlreadyDone),
        Some("processing") => Ok(ProcessingStateAction::AlreadyProcessing),
        Some("failed") => Ok(ProcessingStateAction::AlreadyFailed),
        Some(status) => Err(AppError::Internal(anyhow!(
            "unknown processing_state status: {status}"
        ))),
        None => Err(AppError::Internal(anyhow!(
            "processing_state conflict without existing row"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use chrono::Utc;
    use common::models::{EventType, Source};
    use serde_json::json;

    use super::*;

    async fn insert_workspace(pool: &PgPool, workspace_id: Uuid) {
        let name = format!("workspace-{workspace_id}");
        let result = sqlx::query("INSERT INTO workspaces (id, name, config) VALUES ($1, $2, $3)")
            .bind(workspace_id)
            .bind(name)
            .bind(json!({}))
            .execute(pool)
            .await;

        if let Err(error) = result {
            panic!("test workspace insert should succeed: {error}");
        }
    }

    async fn insert_raw_event(pool: &PgPool, workspace_id: Uuid, raw_event_id: Uuid) {
        let result = sqlx::query(
            r#"
            INSERT INTO raw_events (
                id,
                workspace_id,
                source,
                event_type,
                actor,
                payload,
                idempotency_key,
                occurred_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            "#,
        )
        .bind(raw_event_id)
        .bind(workspace_id)
        .bind(Source::GitHub)
        .bind(EventType::PullRequest)
        .bind("octocat")
        .bind(json!({ "pull_request": { "title": "Title", "body": "Body" } }))
        .bind(format!("github:{raw_event_id}"))
        .bind(Utc::now())
        .execute(pool)
        .await;

        if let Err(error) = result {
            panic!("test raw_event insert should succeed: {error}");
        }
    }

    fn new_memory_unit(workspace_id: Uuid, raw_event_id: Uuid) -> NewMemoryUnit {
        NewMemoryUnit {
            id: Uuid::now_v7(),
            workspace_id,
            scope: json!({
                "workspace_id": workspace_id,
                "source": "github",
                "repo": "Quazmoz/memoryops",
                "actor": "octocat",
                "agent_id": null,
                "user_id": null
            }),
            memory_type: MemoryType::Episodic,
            content: "Useful processor memory".to_owned(),
            entities: json!([]),
            importance_score: 0.75,
            source_events: vec![raw_event_id],
            embedding_id: Some(Uuid::now_v7().to_string()),
            token_count: Some(4),
            tags: Vec::new(),
        }
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn insert_and_retrieve_memory_unit(pool: PgPool) {
        let workspace_id = Uuid::now_v7();
        let raw_event_id = Uuid::now_v7();
        insert_workspace(&pool, workspace_id).await;
        insert_raw_event(&pool, workspace_id, raw_event_id).await;
        let unit = new_memory_unit(workspace_id, raw_event_id);

        let inserted = match insert_memory_unit(&pool, &unit).await {
            Ok(inserted) => inserted,
            Err(error) => panic!("memory unit insert should succeed: {error}"),
        };
        let retrieved = match sqlx::query_as::<_, MemoryUnit>(
            r#"
            SELECT id,
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
            FROM memory_units
            WHERE id = $1
            "#,
        )
        .bind(inserted.id)
        .fetch_one(&pool)
        .await
        {
            Ok(retrieved) => retrieved,
            Err(error) => panic!("memory unit should be retrievable: {error}"),
        };

        assert_eq!(retrieved.id, inserted.id);
        assert_eq!(retrieved.workspace_id, workspace_id);
        assert_eq!(retrieved.content, "Useful processor memory");
        assert_eq!(retrieved.source_events, vec![raw_event_id]);
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn insert_processing_state_conflict_returns_already_done(pool: PgPool) {
        let workspace_id = Uuid::now_v7();
        let raw_event_id = Uuid::now_v7();
        insert_workspace(&pool, workspace_id).await;
        insert_raw_event(&pool, workspace_id, raw_event_id).await;

        let first = match insert_processing_state(&pool, raw_event_id, workspace_id, 600).await {
            Ok(action) => action,
            Err(error) => panic!("processing_state insert should succeed: {error}"),
        };
        if let Err(error) = mark_processing_done(&pool, raw_event_id).await {
            panic!("mark done should succeed: {error}");
        }
        let second = match insert_processing_state(&pool, raw_event_id, workspace_id, 600).await {
            Ok(action) => action,
            Err(error) => panic!("conflict lookup should succeed: {error}"),
        };

        assert_eq!(first, ProcessingStateAction::Proceed);
        assert_eq!(second, ProcessingStateAction::AlreadyDone);
    }
}
