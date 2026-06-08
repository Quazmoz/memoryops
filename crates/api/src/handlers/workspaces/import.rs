use std::str;

use anyhow::anyhow;
use axum::{
    body::Body,
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Extension, Json,
};
use common::{auth::AuthContext, error::AppResult, models::MemoryUnit, AppError, AppState};
use futures_util::StreamExt;
use processor::worker::enqueue_slow_job;
use serde::Serialize;
use sqlx::PgPool;
use uuid::Uuid;

use super::require_workspace;

pub const MAX_IMPORT_BODY_BYTES: usize = 50 * 1024 * 1024;

#[derive(Debug, Default, Serialize)]
pub struct ImportMemoriesResponse {
    /// Memory records successfully upserted into Postgres.
    pub imported: u64,
    /// Records intentionally skipped before persistence. Kept for API compatibility.
    pub skipped: u64,
    /// Records that failed parsing or persistence.
    pub errors: u64,
    /// Imported records successfully enqueued for embedding/re-embedding.
    pub enqueued: u64,
    /// Imported records persisted but not queued for embedding/re-embedding.
    pub enqueue_failed: u64,
}

#[axum::debug_handler]
pub async fn import_memories(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthContext>,
    Path(id): Path<Uuid>,
    body: Body,
) -> Result<impl IntoResponse, AppError> {
    require_workspace(&auth, id)?;
    let workspace_id = auth.workspace_id;
    let mut response = ImportMemoriesResponse::default();
    let mut buffer = String::new();
    let mut stream = body.into_data_stream();

    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|error| AppError::Internal(anyhow!(error)))?;
        let text = str::from_utf8(&chunk).map_err(|_| {
            AppError::Validation("import body must be valid UTF-8 JSONL".to_owned())
        })?;
        buffer.push_str(text);
        if buffer.len() > MAX_IMPORT_BODY_BYTES {
            process_complete_import_lines(&state, workspace_id, &mut buffer, &mut response).await;
            response.errors = response.errors.saturating_add(1);
            return Ok((StatusCode::UNPROCESSABLE_ENTITY, Json(response)));
        }
        process_complete_import_lines(&state, workspace_id, &mut buffer, &mut response).await;
    }

    if !buffer.trim().is_empty() {
        process_import_line(
            &state,
            workspace_id,
            buffer.trim_end_matches('\r'),
            &mut response,
        )
        .await;
    }

    Ok((StatusCode::OK, Json(response)))
}

async fn process_complete_import_lines(
    state: &AppState,
    workspace_id: Uuid,
    buffer: &mut String,
    response: &mut ImportMemoriesResponse,
) {
    while let Some(index) = buffer.find('\n') {
        let line = buffer[..index].trim_end_matches('\r').to_owned();
        buffer.drain(..=index);
        process_import_line(state, workspace_id, &line, response).await;
    }
}

async fn process_import_line(
    state: &AppState,
    workspace_id: Uuid,
    line: &str,
    response: &mut ImportMemoriesResponse,
) {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return;
    }

    let memory = match serde_json::from_str::<MemoryUnit>(trimmed) {
        Ok(memory) => memory,
        Err(error) => {
            response.errors = response.errors.saturating_add(1);
            tracing::warn!(error = ?error, "failed to parse JSONL memory import line");
            return;
        }
    };

    let memory = sanitize_imported_memory(memory, workspace_id);
    match upsert_imported_memory(&state.db, &memory).await {
        Ok(memory_id) => {
            response.imported = response.imported.saturating_add(1);
            match state.redis.get().await {
                Ok(mut conn) => {
                    if let Err(error) =
                        enqueue_slow_job(&mut *conn, memory_id, workspace_id, 0).await
                    {
                        response.enqueue_failed = response.enqueue_failed.saturating_add(1);
                        tracing::warn!(error = ?error, memory_id = %memory_id, "failed to enqueue imported memory for embedding");
                    } else {
                        response.enqueued = response.enqueued.saturating_add(1);
                    }
                }
                Err(error) => {
                    response.enqueue_failed = response.enqueue_failed.saturating_add(1);
                    tracing::warn!(error = ?error, memory_id = %memory_id, "failed to get Redis connection for import enqueue")
                }
            }
        }
        Err(error) => {
            response.errors = response.errors.saturating_add(1);
            tracing::warn!(error = ?error, memory_id = %memory.id, "failed to upsert imported memory");
        }
    }
}

pub(crate) fn sanitize_imported_memory(mut memory: MemoryUnit, workspace_id: Uuid) -> MemoryUnit {
    memory.workspace_id = workspace_id;
    memory.scope.workspace_id = workspace_id;
    memory.embedding_id = None;
    memory.deleted_at = None;
    // Reset promotion state so the memory goes through the normal promotion
    // pipeline in the target workspace rather than arriving pre-promoted.
    memory.promoted_at = None;
    // Reset corroboration count so inflated weights from the source workspace
    // don't carry over without the corresponding source events being present.
    memory.corroboration_count = 0;
    // Reset version so conflict resolution starts clean in the target workspace.
    memory.version = 1;
    // Retrieval feedback is workspace-local; imported memories should start neutral.
    memory.relevance_score = 0.5;
    memory
}

async fn upsert_imported_memory(db: &PgPool, memory: &MemoryUnit) -> AppResult<Uuid> {
    let scope =
        serde_json::to_value(&memory.scope).map_err(|error| AppError::Internal(anyhow!(error)))?;
    let entities = serde_json::to_value(&memory.entities.0)
        .map_err(|error| AppError::Internal(anyhow!(error)))?;

    // Import behaves as restore/upsert rather than append-only load. Re-importing
    // the same exported memory ID updates that record in the target workspace.
    sqlx::query_scalar::<_, Uuid>(
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
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, NULL, $11, $12, $13, $14, $15, $16, $17, $18, $19, NULL, $20, $21, $22)
        ON CONFLICT (id) DO UPDATE SET
            workspace_id = EXCLUDED.workspace_id,
            scope = EXCLUDED.scope,
            memory_type = EXCLUDED.memory_type,
            scope_visibility = EXCLUDED.scope_visibility,
            content = EXCLUDED.content,
            entities = EXCLUDED.entities,
            importance_score = EXCLUDED.importance_score,
            importance_overridden = EXCLUDED.importance_overridden,
            source_events = EXCLUDED.source_events,
            embedding_id = NULL,
            token_count = EXCLUDED.token_count,
            decay_score = EXCLUDED.decay_score,
            relevance_score = EXCLUDED.relevance_score,
            pinned = EXCLUDED.pinned,
            tags = EXCLUDED.tags,
            version = EXCLUDED.version,
            promoted_at = EXCLUDED.promoted_at,
            source_episode_ids = EXCLUDED.source_episode_ids,
            corroboration_count = EXCLUDED.corroboration_count,
            deleted_at = NULL,
            last_accessed_at = EXCLUDED.last_accessed_at,
            updated_at = now()
        RETURNING id
        "#,
    )
    .bind(memory.id)
    .bind(memory.workspace_id)
    .bind(scope)
    .bind(memory.memory_type)
    .bind(memory.scope_visibility)
    .bind(&memory.content)
    .bind(entities)
    .bind(memory.importance_score)
    .bind(memory.importance_overridden)
    .bind(&memory.source_events)
    .bind(memory.token_count)
    .bind(memory.decay_score)
    .bind(memory.relevance_score)
    .bind(memory.pinned)
    .bind(&memory.tags)
    .bind(memory.version)
    .bind(memory.promoted_at)
    .bind(&memory.source_episode_ids)
    .bind(memory.corroboration_count)
    .bind(memory.last_accessed_at)
    .bind(memory.created_at)
    .bind(memory.updated_at)
    .fetch_one(db)
    .await
    .map_err(AppError::Database)
}
