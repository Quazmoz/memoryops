use std::{
    net::{IpAddr, SocketAddr},
    str,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use anyhow::anyhow;
use axum::{
    body::Body,
    extract::{Path, Query, State},
    http::{header::HeaderName, HeaderMap, StatusCode},
    response::IntoResponse,
    Extension, Json,
};
use chrono::{DateTime, NaiveDate, Utc};
use common::{
    audit::spawn_audit_log,
    auth::AuthContext,
    error::AppResult,
    models::{AuditAction, ContradictionMode, MemoryUnit, Workspace, WorkspaceConfig},
    AppError, AppState,
};
use futures_util::StreamExt;
use processor::embedder::COLLECTION_NAME;
use processor::worker::enqueue_slow_job;
use processor::{
    config::fetch_workspace_promotion_config,
    promoter::{run_promotion_pass, PromotionReport},
};
use qdrant_client::qdrant::DeletePointsBuilder;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::PgPool;
use uuid::Uuid;

use super::{keys, require_workspace};

pub const MAX_IMPORT_BODY_BYTES: usize = 50 * 1024 * 1024;
const WORKSPACE_CREATE_RATE_LIMIT_CAPACITY: i64 = 5;
const WORKSPACE_CREATE_REFILL_TOKENS_PER_SEC: f64 = 5.0 / 3600.0;
const X_ADMIN_TOKEN_HEADER: HeaderName = HeaderName::from_static("x-admin-token");
const X_FORWARDED_FOR_HEADER: HeaderName = HeaderName::from_static("x-forwarded-for");
const KNOWN_CONFIG_KEYS: &[&str] = &[
    "promotion_threshold",
    "dedup_cosine_threshold",
    "access_count_trigger",
    "half_life_days",
    "decay_rate_episodic",
    "decay_rate_semantic",
    "llm_provider",
    "embedding_provider",
    "llm_model",
    "embedding_model",
    "decay_half_life_days",
    "pruning_threshold",
    "retention_max_age_days",
    "compliance_hard_purge",
    "contradiction_mode",
    "contradiction_threshold",
    "contradiction_candidates",
    "sub_agent_pools",
];

#[derive(Debug, Deserialize)]
pub struct CreateWorkspaceRequest {
    pub name: String,
    pub config: Option<WorkspaceConfig>,
}

#[derive(Debug, Serialize)]
pub struct CreateWorkspaceResponse {
    pub workspace_id: Uuid,
    pub api_key: String,
}

#[derive(Debug, Deserialize)]
pub struct UpdateWorkspaceConfigRequest {
    pub promotion_threshold: Option<f32>,
    pub dedup_cosine_threshold: Option<f32>,
    pub llm_provider: Option<String>,
    pub llm_model: Option<String>,
    pub embedding_provider: Option<String>,
    pub embedding_model: Option<String>,
    pub decay_half_life_days: Option<u32>,
    pub pruning_threshold: Option<f32>,
    #[serde(default)]
    pub retention_max_age_days: Option<Option<u32>>,
    pub compliance_hard_purge: Option<bool>,
    pub contradiction_mode: Option<ContradictionMode>,
    pub contradiction_threshold: Option<f32>,
    pub contradiction_candidates: Option<usize>,
    pub sub_agent_pools: Option<Vec<String>>,
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

#[derive(Debug, Serialize)]
pub struct WorkspaceStats {
    pub total_memories: i64,
    pub episodic_count: i64,
    pub semantic_count: i64,
    pub pinned_count: i64,
    pub deleted_count: i64,
    pub avg_importance_score: f64,
    pub avg_decay_score: f64,
    pub memories_created_7d: i64,
    pub memories_created_30d: i64,
    pub oldest_memory_at: Option<DateTime<Utc>>,
    pub newest_memory_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Deserialize)]
pub struct StatsHistoryQuery {
    pub days: Option<u32>,
}

#[derive(Debug, Serialize)]
pub struct WorkspaceStatsHistory {
    pub days: u32,
    pub series: Vec<WorkspaceStatsHistoryPoint>,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct WorkspaceStatsHistoryPoint {
    pub date: NaiveDate,
    pub created: i64,
    pub promoted: i64,
    pub soft_deleted: i64,
}

#[derive(Debug, Default, Serialize)]
pub struct ImportMemoriesResponse {
    pub imported: u64,
    pub skipped: u64,
    pub errors: u64,
}

#[derive(Debug, sqlx::FromRow)]
struct WorkspaceStatsRow {
    total_memories: i64,
    episodic_count: i64,
    semantic_count: i64,
    pinned_count: i64,
    deleted_count: i64,
    avg_importance_score: Option<f64>,
    avg_decay_score: Option<f64>,
    memories_created_7d: i64,
    memories_created_30d: i64,
    oldest_memory_at: Option<DateTime<Utc>>,
    newest_memory_at: Option<DateTime<Utc>>,
}

pub async fn create_workspace(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<CreateWorkspaceRequest>,
) -> AppResult<Json<CreateWorkspaceResponse>> {
    authorize_workspace_creation(&headers)?;
    let created_from_ip = resolve_client_ip(&headers);
    enforce_workspace_creation_rate_limit(&state, created_from_ip).await?;

    if request.name.trim().is_empty() {
        return Err(AppError::Validation(
            "workspace name is required".to_owned(),
        ));
    }

    let workspace_id = Uuid::now_v7();
    let config = request.config.unwrap_or_default();
    let config_value =
        serde_json::to_value(&config).map_err(|error| AppError::Internal(anyhow!(error)))?;

    sqlx::query(
        r#"
        INSERT INTO workspaces (id, name, config, promotion_threshold, dedup_cosine_threshold, created_from_ip)
        VALUES ($1, $2, $3, $4, $5, $6)
        "#,
    )
    .bind(workspace_id)
    .bind(request.name.trim())
    .bind(config_value)
    .bind(config.promotion_threshold)
    .bind(config.dedup_cosine_threshold)
    .bind(created_from_ip.to_string())
    .execute(&state.db)
    .await
    .map_err(|error| {
        if let sqlx::Error::Database(ref db_err) = error {
            if db_err.code().as_deref() == Some("23505") {
                return AppError::Conflict(format!(
                    "workspace with name '{}' already exists",
                    request.name.trim()
                ));
            }
        }
        AppError::Database(error)
    })?;

    let (api_key, _record) = keys::insert_key(&state.db, workspace_id, "bootstrap").await?;

    Ok(Json(CreateWorkspaceResponse {
        workspace_id,
        api_key,
    }))
}

#[axum::debug_handler]
pub async fn list_workspaces(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthContext>,
) -> AppResult<Json<Workspace>> {
    // TODO: if/when multi-workspace accounts are introduced, add a dedicated list endpoint.
    let workspace = get_workspace_by_id(&state, auth.workspace_id)
        .await?
        .ok_or_else(|| AppError::NotFound {
            resource: format!("workspace:{}", auth.workspace_id),
        })?;
    Ok(Json(workspace))
}

pub async fn get_workspace(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthContext>,
    Path(id): Path<Uuid>,
) -> AppResult<Json<Workspace>> {
    require_workspace(&auth, id)?;
    let workspace = get_workspace_by_id(&state, id)
        .await?
        .ok_or_else(|| AppError::NotFound {
            resource: format!("workspace:{id}"),
        })?;

    Ok(Json(workspace))
}

#[axum::debug_handler]
pub async fn get_stats(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthContext>,
    Path(id): Path<Uuid>,
) -> AppResult<Json<WorkspaceStats>> {
    require_workspace(&auth, id)?;
    let row = sqlx::query_as::<_, WorkspaceStatsRow>(
        r#"
        SELECT
            COUNT(*) FILTER (WHERE deleted_at IS NULL) AS total_memories,
            COUNT(*) FILTER (WHERE deleted_at IS NULL AND memory_type = 'episodic') AS episodic_count,
            COUNT(*) FILTER (WHERE deleted_at IS NULL AND memory_type = 'semantic') AS semantic_count,
            COUNT(*) FILTER (WHERE deleted_at IS NULL AND pinned) AS pinned_count,
            COUNT(*) FILTER (WHERE deleted_at IS NOT NULL AND hard_deleted_at IS NULL) AS deleted_count,
            AVG(importance_score) FILTER (WHERE deleted_at IS NULL) AS avg_importance_score,
            AVG(decay_score) FILTER (WHERE deleted_at IS NULL) AS avg_decay_score,
            COUNT(*) FILTER (WHERE deleted_at IS NULL AND created_at >= NOW() - INTERVAL '7 days') AS memories_created_7d,
            COUNT(*) FILTER (WHERE deleted_at IS NULL AND created_at >= NOW() - INTERVAL '30 days') AS memories_created_30d,
            MIN(created_at) FILTER (WHERE deleted_at IS NULL) AS oldest_memory_at,
            MAX(created_at) FILTER (WHERE deleted_at IS NULL) AS newest_memory_at
        FROM memory_units
        WHERE workspace_id = $1
        "#,
    )
    .bind(id)
    .fetch_one(&state.db)
    .await
    .map_err(AppError::Database)?;

    Ok(Json(workspace_stats_from_row(row)))
}

#[axum::debug_handler]
pub async fn get_stats_history(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthContext>,
    Path(id): Path<Uuid>,
    Query(query): Query<StatsHistoryQuery>,
) -> AppResult<Json<WorkspaceStatsHistory>> {
    require_workspace(&auth, id)?;
    let days = stats_history_days(query.days)?;
    let rows = sqlx::query_as::<_, WorkspaceStatsHistoryPoint>(
        r#"
        WITH bounds AS (
            SELECT
                (TIMEZONE('UTC', NOW())::DATE - (($2::INTEGER - 1) * INTERVAL '1 day'))::DATE AS start_date,
                TIMEZONE('UTC', NOW())::DATE AS end_date
        ),
        dates AS (
            SELECT GENERATE_SERIES(start_date, end_date, INTERVAL '1 day')::DATE AS date
            FROM bounds
        ),
        created AS (
            SELECT DATE(created_at AT TIME ZONE 'UTC') AS date, COUNT(*) AS created
            FROM memory_units
            WHERE workspace_id = $1
            GROUP BY 1
        ),
        promoted AS (
                        SELECT DATE(promoted_at AT TIME ZONE 'UTC') AS date, COUNT(*) AS promoted
            FROM memory_units
            WHERE workspace_id = $1
                            AND promoted_at IS NOT NULL
            GROUP BY 1
        ),
        soft_deleted AS (
            SELECT DATE(deleted_at AT TIME ZONE 'UTC') AS date, COUNT(*) AS soft_deleted
            FROM memory_units
            WHERE workspace_id = $1
              AND deleted_at IS NOT NULL
            GROUP BY 1
        )
        SELECT
            dates.date,
            COALESCE(created.created, 0)::BIGINT AS created,
            COALESCE(promoted.promoted, 0)::BIGINT AS promoted,
            COALESCE(soft_deleted.soft_deleted, 0)::BIGINT AS soft_deleted
        FROM dates
        LEFT JOIN created ON created.date = dates.date
        LEFT JOIN promoted ON promoted.date = dates.date
        LEFT JOIN soft_deleted ON soft_deleted.date = dates.date
        ORDER BY dates.date ASC
        "#,
    )
    // FIX: was `auth.workspace_id` — must use path `id` so the query returns
    // data for the requested workspace, not always the caller's own workspace.
    .bind(id)
    .bind(i64::from(days))
    .fetch_all(&state.db)
    .await
    .map_err(AppError::Database)?;

    Ok(Json(WorkspaceStatsHistory { days, series: rows }))
}

#[axum::debug_handler]
pub async fn update_workspace_config(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthContext>,
    Path(id): Path<Uuid>,
    Json(config): Json<UpdateWorkspaceConfigRequest>,
) -> AppResult<Json<Workspace>> {
    require_workspace(&auth, id)?;
    validate_threshold("promotion_threshold", config.promotion_threshold, 0.5, 1.0)?;
    validate_threshold(
        "dedup_cosine_threshold",
        config.dedup_cosine_threshold,
        0.80,
        0.99,
    )?;
    validate_lifecycle_config(&config)?;
    validate_compliance_config(&config)?;
    validate_contradiction_config(&config)?;
    validate_sub_agent_pools(&config)?;

    let before = get_workspace_by_id(&state, id)
        .await?
        .ok_or_else(|| AppError::NotFound {
            resource: format!("workspace:{id}"),
        })?;
    let mut config_value = before.config.clone();
    merge_workspace_config(&mut config_value, &config)?;
    let promotion_threshold = config
        .promotion_threshold
        .unwrap_or(before.promotion_threshold);
    let dedup_cosine_threshold = config
        .dedup_cosine_threshold
        .unwrap_or(before.dedup_cosine_threshold);
    let updated = sqlx::query_as::<_, Workspace>(
        r#"
        UPDATE workspaces
        SET config = $2,
            promotion_threshold = $3,
            dedup_cosine_threshold = $4
        WHERE id = $1 AND deleted_at IS NULL
        RETURNING id,
                  name,
                  config,
                  promotion_threshold::REAL AS promotion_threshold,
                  dedup_cosine_threshold::REAL AS dedup_cosine_threshold,
                  created_at,
                  updated_at,
                  deleted_at
        "#,
    )
    .bind(id)
    .bind(config_value)
    .bind(promotion_threshold)
    .bind(dedup_cosine_threshold)
    .fetch_optional(&state.db)
    .await
    .map_err(AppError::Database)?
    .ok_or_else(|| AppError::NotFound {
        resource: format!("workspace:{id}"),
    })?;

    spawn_audit_log(
        state.db.clone(),
        id,
        auth.actor(),
        AuditAction::ConfigUpdated,
        id,
        "workspace",
        Some(json!({ "before": before.config, "after": updated.config })),
    );

    Ok(Json(updated))
}

#[axum::debug_handler]
pub async fn delete_workspace(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthContext>,
    Path(id): Path<Uuid>,
) -> AppResult<Json<serde_json::Value>> {
    require_workspace(&auth, id)?;

    let deleted = sqlx::query(
        r#"
        UPDATE workspaces
        SET deleted_at = NOW(), updated_at = NOW()
        WHERE id = $1 AND deleted_at IS NULL
        "#,
    )
    .bind(id)
    .execute(&state.db)
    .await
    .map_err(AppError::Database)?
    .rows_affected();

    if deleted == 0 {
        return Err(AppError::NotFound {
            resource: format!("workspace:{id}"),
        });
    }

    spawn_audit_log(
        state.db.clone(),
        id,
        auth.actor(),
        AuditAction::WorkspaceDeleted,
        id,
        "workspace",
        None,
    );

    enqueue_workspace_purge_job(state.clone(), id);

    Ok(Json(json!({ "deleted": true })))
}

#[axum::debug_handler]
pub async fn promote(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthContext>,
    Path(id): Path<Uuid>,
) -> AppResult<Json<PromotionReport>> {
    require_workspace(&auth, id)?;
    let lock_key = format!("promotion:lock:{id}");
    let lock_token = acquire_promotion_lock(&state, &lock_key).await?;

    let config = fetch_workspace_promotion_config(&state.db, id).await?;
    let result = tokio::time::timeout(
        Duration::from_secs(60),
        run_promotion_pass(
            &state.db,
            &state.qdrant,
            state.llm_provider.as_ref(),
            state.embedding_provider.as_ref(),
            id,
            config,
        ),
    )
    .await;

    release_promotion_lock(&state, &lock_key, &lock_token).await;

    let report = match result {
        Ok(Ok(report)) => report,
        Ok(Err(error)) => return Err(AppError::Internal(error)),
        Err(error) => return Err(AppError::Internal(anyhow!(error))),
    };

    spawn_audit_log(
        state.db.clone(),
        id,
        auth.actor(),
        AuditAction::WorkspacePromote,
        id,
        "workspace",
        Some(json!({
            "clusters_found": report.clusters_found,
            "units_promoted": report.units_promoted,
            "units_skipped": report.units_skipped
        })),
    );

    Ok(Json(report))
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

    if should_skip_import(&memory, workspace_id) {
        response.skipped = response.skipped.saturating_add(1);
        return;
    }

    let memory = sanitize_imported_memory(memory, workspace_id);
    match upsert_imported_memory(&state.db, &memory).await {
        Ok(memory_id) => {
            response.imported = response.imported.saturating_add(1);
            match state.redis.get().await {
                Ok(mut conn) => {
                    if let Err(error) = enqueue_slow_job(&mut *conn, memory_id, workspace_id, 0).await {
                        tracing::warn!(error = ?error, memory_id = %memory_id, "failed to enqueue imported memory for embedding");
                    }
                }
                Err(error) => tracing::warn!(error = ?error, memory_id = %memory_id, "failed to get Redis connection for import enqueue"),
            }
        }
        Err(error) => {
            response.errors = response.errors.saturating_add(1);
            tracing::warn!(error = ?error, memory_id = %memory.id, "failed to upsert imported memory");
        }
    }
}

fn should_skip_import(memory: &MemoryUnit, workspace_id: Uuid) -> bool {
    memory.workspace_id != workspace_id
}

fn sanitize_imported_memory(mut memory: MemoryUnit, workspace_id: Uuid) -> MemoryUnit {
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
    memory
}

async fn upsert_imported_memory(db: &PgPool, memory: &MemoryUnit) -> AppResult<Uuid> {
    let scope =
        serde_json::to_value(&memory.scope).map_err(|error| AppError::Internal(anyhow!(error)))?;
    let entities = serde_json::to_value(&memory.entities.0)
        .map_err(|error| AppError::Internal(anyhow!(error)))?;

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

async fn get_workspace_by_id(state: &AppState, id: Uuid) -> AppResult<Option<Workspace>> {
    sqlx::query_as::<_, Workspace>(
        r#"
        SELECT id,
               name,
               config,
               promotion_threshold::REAL AS promotion_threshold,
               dedup_cosine_threshold::REAL AS dedup_cosine_threshold,
               created_at,
               updated_at,
               deleted_at
        FROM workspaces
        WHERE id = $1 AND deleted_at IS NULL
        "#,
    )
    .bind(id)
    .fetch_optional(&state.db)
    .await
    .map_err(AppError::Database)
}

fn workspace_stats_from_row(row: WorkspaceStatsRow) -> WorkspaceStats {
    WorkspaceStats {
        total_memories: row.total_memories,
        episodic_count: row.episodic_count,
        semantic_count: row.semantic_count,
        pinned_count: row.pinned_count,
        deleted_count: row.deleted_count,
        avg_importance_score: row.avg_importance_score.unwrap_or(0.0),
        avg_decay_score: row.avg_decay_score.unwrap_or(0.0),
        memories_created_7d: row.memories_created_7d,
        memories_created_30d: row.memories_created_30d,
        oldest_memory_at: row.oldest_memory_at,
        newest_memory_at: row.newest_memory_at,
    }
}

fn stats_history_days(days: Option<u32>) -> AppResult<u32> {
    let days = days.unwrap_or(30);

    if days == 0 {
        return Err(AppError::Validation("days must be at least 1".to_owned()));
    }

    if days > 90 {
        return Err(AppError::Validation(
            "days must be less than or equal to 90".to_owned(),
        ));
    }

    Ok(days)
}

async fn acquire_promotion_lock(state: &AppState, key: &str) -> AppResult<String> {
    let mut redis = state
        .redis
        .get()
        .await
        .map_err(|error| AppError::Internal(anyhow!(error)))?;
    let token = Uuid::new_v4().to_string();
    let acquired = redis::cmd("SET")
        .arg(key)
        .arg(&token)
        .arg("NX")
        .arg("EX")
        .arg(600)
        .query_async::<Option<String>>(&mut *redis)
        .await
        .map_err(|error| AppError::Internal(anyhow!(error)))?
        .is_some();

    if acquired {
        Ok(token)
    } else {
        Err(AppError::Conflict(
            "promotion already running for this workspace".to_owned(),
        ))
    }
}

async fn release_promotion_lock(state: &AppState, key: &str, token: &str) {
    let mut redis = match state.redis.get().await {
        Ok(conn) => conn,
        Err(error) => {
            tracing::warn!(error = ?error, key, "failed to get Redis connection to release promotion lock");
            return;
        }
    };
    let script = redis::Script::new(
        r#"
        if redis.call("get", KEYS[1]) == ARGV[1] then
          return redis.call("del", KEYS[1])
        else
          return 0
        end
        "#,
    );
    if let Err(error) = script
        .key(key)
        .arg(token)
        .invoke_async::<i64>(&mut *redis)
        .await
    {
        tracing::warn!(error = ?error, key, "failed to release promotion lock");
    }
}

fn merge_workspace_config(
    target: &mut serde_json::Value,
    patch: &UpdateWorkspaceConfigRequest,
) -> AppResult<()> {
    if !target.is_object() {
        *target = json!({});
    }

    let Some(object) = target.as_object_mut() else {
        return Ok(());
    };

    if let Some(value) = patch.promotion_threshold {
        object.insert("promotion_threshold".to_owned(), json!(value));
    }
    if let Some(value) = patch.dedup_cosine_threshold {
        object.insert("dedup_cosine_threshold".to_owned(), json!(value));
    }
    if let Some(value) = &patch.llm_provider {
        object.insert("llm_provider".to_owned(), json!(value));
    }
    if let Some(value) = &patch.llm_model {
        object.insert("llm_model".to_owned(), json!(value));
    }
    if let Some(value) = &patch.embedding_provider {
        object.insert("embedding_provider".to_owned(), json!(value));
    }
    if let Some(value) = &patch.embedding_model {
        object.insert("embedding_model".to_owned(), json!(value));
    }
    if let Some(value) = patch.decay_half_life_days {
        object.insert("decay_half_life_days".to_owned(), json!(value));
    }
    if let Some(value) = patch.pruning_threshold {
        object.insert("pruning_threshold".to_owned(), json!(value));
    }
    match patch.retention_max_age_days {
        Some(Some(value)) => {
            object.insert("retention_max_age_days".to_owned(), json!(value));
        }
        Some(None) => {
            object.remove("retention_max_age_days");
        }
        None => {}
    }
    if let Some(value) = patch.compliance_hard_purge {
        object.insert("compliance_hard_purge".to_owned(), json!(value));
    }
    if let Some(value) = patch.contradiction_mode {
        object.insert("contradiction_mode".to_owned(), json!(value));
    }
    if let Some(value) = patch.contradiction_threshold {
        object.insert("contradiction_threshold".to_owned(), json!(value));
    }
    if let Some(value) = patch.contradiction_candidates {
        object.insert("contradiction_candidates".to_owned(), json!(value));
    }
    if let Some(value) = &patch.sub_agent_pools {
        object.insert(
            "sub_agent_pools".to_owned(),
            json!(normalized_sub_agent_pools(value)),
        );
    }
    for key in patch.extra.keys() {
        if KNOWN_CONFIG_KEYS.contains(&key.as_str()) {
            return Err(AppError::Validation(format!(
                "config field '{key}' must be set via the typed request field"
            )));
        }

        tracing::warn!(
            key = %key,
            "ignoring unknown workspace config patch field"
        );
    }

    Ok(())
}

fn authorize_workspace_creation(headers: &HeaderMap) -> AppResult<()> {
    let expected_secret = std::env::var("WORKSPACE_CREATION_SECRET")
        .map_err(|_| AppError::Internal(anyhow!("WORKSPACE_CREATION_SECRET is not configured")))?;

    let Some(provided_header) = headers.get(&X_ADMIN_TOKEN_HEADER) else {
        return Err(AppError::Forbidden);
    };

    let Ok(provided_secret) = provided_header.to_str() else {
        return Err(AppError::Forbidden);
    };

    if provided_secret == expected_secret {
        Ok(())
    } else {
        Err(AppError::Forbidden)
    }
}

fn resolve_client_ip(headers: &HeaderMap) -> IpAddr {
    // 1. X-Forwarded-For (set by nginx and most CDNs)
    if let Some(value) = headers.get(&X_FORWARDED_FOR_HEADER) {
        if let Ok(raw) = value.to_str() {
            if let Some(first) = raw.split(',').next() {
                let candidate = first.trim();
                if let Ok(ip) = candidate.parse::<IpAddr>() {
                    return ip;
                }
                if let Ok(addr) = candidate.parse::<SocketAddr>() {
                    return addr.ip();
                }
            }
        }
    }

    // 2. X-Real-IP (set by some reverse proxies)
    if let Some(value) = headers.get("x-real-ip") {
        if let Ok(raw) = value.to_str() {
            if let Ok(ip) = raw.trim().parse::<IpAddr>() {
                return ip;
            }
        }
    }

    // 3. Local dev fallback — rate-limiting still applies but all local
    //    requests share the same bucket, which is fine for development.
    IpAddr::from([127, 0, 0, 1])
}

async fn enforce_workspace_creation_rate_limit(state: &AppState, ip: IpAddr) -> AppResult<()> {
    let key = format!("workspace:create:ratelimit:{ip}");
    let now = unix_timestamp_secs()?;

    let mut redis = state
        .redis
        .get()
        .await
        .map_err(|error| AppError::Internal(anyhow!(error)))?;
    let allowed = redis::Script::new(
        r#"
        local data = redis.call('HMGET', KEYS[1], 'tokens', 'ts')
        local tokens = tonumber(data[1])
        local ts = tonumber(data[2])
        local now = tonumber(ARGV[1])
        local capacity = tonumber(ARGV[2])
        local refill = tonumber(ARGV[3])

        if tokens == nil then
          tokens = capacity
          ts = now
        end

        local elapsed = math.max(0, now - ts)
        tokens = math.min(capacity, tokens + (elapsed * refill))

        local allowed = 0
        if tokens >= 1 then
          tokens = tokens - 1
          allowed = 1
        end

        redis.call('HMSET', KEYS[1], 'tokens', tokens, 'ts', now)
        redis.call('EXPIRE', KEYS[1], 7200)
        return allowed
        "#,
    )
    .key(&key)
    .arg(now)
    .arg(WORKSPACE_CREATE_RATE_LIMIT_CAPACITY)
    .arg(WORKSPACE_CREATE_REFILL_TOKENS_PER_SEC)
    .invoke_async::<i64>(&mut *redis)
    .await
    .map_err(|error| AppError::Internal(anyhow!(error)))?;

    if allowed == 1 {
        Ok(())
    } else {
        Err(AppError::RateLimited {
            retry_after_secs: 3600,
        })
    }
}

fn unix_timestamp_secs() -> AppResult<i64> {
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| AppError::Internal(anyhow!(error)))?;
    i64::try_from(duration.as_secs()).map_err(|error| AppError::Internal(anyhow!(error)))
}

fn enqueue_workspace_purge_job(state: AppState, workspace_id: Uuid) {
    tokio::spawn(async move {
        if let Err(error) = purge_workspace_associations(&state, workspace_id).await {
            tracing::warn!(error = ?error, workspace_id = %workspace_id, "workspace purge job failed");
        }
    });
}

async fn purge_workspace_associations(state: &AppState, workspace_id: Uuid) -> AppResult<()> {
    #[derive(sqlx::FromRow)]
    struct WorkspaceEmbeddingRow {
        id: Uuid,
    }

    let embedded_ids = sqlx::query_as::<_, WorkspaceEmbeddingRow>(
        r#"
        SELECT id
        FROM memory_units
        WHERE workspace_id = $1
          AND embedding_id IS NOT NULL
        "#,
    )
    .bind(workspace_id)
    .fetch_all(&state.db)
    .await
    .map_err(AppError::Database)?;

    for row in &embedded_ids {
        if let Err(error) = state
            .qdrant
            .delete_points(
                DeletePointsBuilder::new(COLLECTION_NAME)
                    .points([row.id.to_string()])
                    .wait(true),
            )
            .await
        {
            tracing::warn!(
                error = ?error,
                workspace_id = %workspace_id,
                memory_id = %row.id,
                "failed to delete workspace point from Qdrant"
            );
        }
    }

    sqlx::query(
        r#"
        UPDATE memory_units
        SET deleted_at = NOW(),
            embedding_id = NULL,
            updated_at = NOW()
        WHERE workspace_id = $1
          AND deleted_at IS NULL
        "#,
    )
    .bind(workspace_id)
    .execute(&state.db)
    .await
    .map_err(AppError::Database)?;

    sqlx::query(
        r#"
        UPDATE api_keys
        SET revoked = TRUE,
            revoked_at = COALESCE(revoked_at, NOW())
        WHERE workspace_id = $1
          AND revoked = FALSE
        "#,
    )
    .bind(workspace_id)
    .execute(&state.db)
    .await
    .map_err(AppError::Database)?;

    Ok(())
}

fn validate_sub_agent_pools(config: &UpdateWorkspaceConfigRequest) -> AppResult<()> {
    let Some(agent_ids) = &config.sub_agent_pools else {
        return Ok(());
    };

    if agent_ids.len() > 100 {
        return Err(AppError::Validation(
            "sub_agent_pools is limited to 100 agent ids".to_owned(),
        ));
    }

    if agent_ids.iter().any(|agent_id| agent_id.trim().is_empty()) {
        return Err(AppError::Validation(
            "sub_agent_pools cannot contain empty agent ids".to_owned(),
        ));
    }

    Ok(())
}

fn normalized_sub_agent_pools(agent_ids: &[String]) -> Vec<String> {
    let mut normalized = Vec::new();
    for agent_id in agent_ids {
        let trimmed = agent_id.trim();
        if normalized.iter().any(|existing| existing == trimmed) {
            continue;
        }
        normalized.push(trimmed.to_owned());
    }
    normalized
}

fn validate_contradiction_config(config: &UpdateWorkspaceConfigRequest) -> AppResult<()> {
    if let Some(threshold) = config.contradiction_threshold {
        if !threshold.is_finite() || !(0.0..=1.0).contains(&threshold) {
            return Err(AppError::Validation(
                "contradiction_threshold must be between 0.0 and 1.0".to_owned(),
            ));
        }
    }

    if let Some(candidates) = config.contradiction_candidates {
        if !(1..=200).contains(&candidates) {
            return Err(AppError::Validation(
                "contradiction_candidates must be between 1 and 200".to_owned(),
            ));
        }
    }

    Ok(())
}

fn validate_lifecycle_config(config: &UpdateWorkspaceConfigRequest) -> AppResult<()> {
    if let Some(days) = config.decay_half_life_days {
        if !(1..=3650).contains(&days) {
            return Err(AppError::Validation(
                "decay_half_life_days must be between 1 and 3650".to_owned(),
            ));
        }
    }

    if let Some(threshold) = config.pruning_threshold {
        if !threshold.is_finite() || !(0.01..=0.50).contains(&threshold) {
            return Err(AppError::Validation(
                "pruning_threshold must be between 0.01 and 0.50".to_owned(),
            ));
        }
    }

    Ok(())
}

fn validate_compliance_config(config: &UpdateWorkspaceConfigRequest) -> AppResult<()> {
    if let Some(Some(days)) = config.retention_max_age_days {
        if !(1..=3650).contains(&days) {
            return Err(AppError::Validation(
                "retention_max_age_days must be between 1 and 3650".to_owned(),
            ));
        }
    }

    Ok(())
}

fn validate_threshold(
    field: &'static str,
    value: Option<f32>,
    min: f32,
    max: f32,
) -> AppResult<()> {
    let Some(value) = value else {
        return Ok(());
    };

    if value.is_finite() && value >= min && value <= max {
        Ok(())
    } else {
        Err(AppError::Validation(format!(
            "{field} must be between {min:.2} and {max:.2}"
        )))
    }
}

const REINDEX_PAGE_SIZE: i64 = 1000;

#[derive(Debug, Deserialize)]
pub struct ReindexQuery {
    pub force: Option<bool>,
    pub after: Option<Uuid>,
}

#[derive(Debug, Serialize)]
pub struct ReindexResponse {
    pub enqueued: usize,
    pub next_cursor: Option<Uuid>,
}

#[axum::debug_handler]
pub async fn reindex_workspace(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthContext>,
    Path(id): Path<Uuid>,
    Query(query): Query<ReindexQuery>,
) -> AppResult<Json<ReindexResponse>> {
    require_workspace(&auth, id)?;

    let force = query.force.unwrap_or(false);

    // If force mode: clear embedding_ids for all non-deleted memories first
    if force {
        sqlx::query(
            "UPDATE memory_units SET embedding_id = NULL WHERE workspace_id = $1 AND deleted_at IS NULL AND ($2::UUID IS NULL OR id > $2)",
        )
        .bind(id)
        .bind(query.after)
        .execute(&state.db)
        .await
        .map_err(AppError::Database)?;
    }

    // Fetch memories that need embedding (missing embedding_id), with cursor pagination
    #[derive(sqlx::FromRow)]
    struct MemoryIdRow {
        id: Uuid,
        workspace_id: Uuid,
    }

    let mut rows: Vec<MemoryIdRow> = sqlx::query_as::<_, MemoryIdRow>(
        r#"
        SELECT id, workspace_id
        FROM memory_units
        WHERE workspace_id = $1
          AND deleted_at IS NULL
          AND embedding_id IS NULL
          AND ($2::UUID IS NULL OR id > $2)
        ORDER BY id ASC
        LIMIT $3
        "#,
    )
    .bind(id)
    .bind(query.after)
    .bind(REINDEX_PAGE_SIZE + 1)
    .fetch_all(&state.db)
    .await
    .map_err(AppError::Database)?;

    let next_cursor = if rows.len() > REINDEX_PAGE_SIZE as usize {
        rows.pop().map(|row| row.id)
    } else {
        None
    };

    let enqueued = rows.len();
    match state.redis.get().await {
        Ok(mut conn) => {
            for row in &rows {
                if let Err(error) = enqueue_slow_job(&mut *conn, row.id, row.workspace_id, 0).await {
                    tracing::warn!(error = ?error, memory_id = %row.id, "failed to enqueue memory for reindex");
                }
            }
        }
        Err(error) => tracing::warn!(error = ?error, "failed to get Redis connection for reindex enqueue"),
    }

    // FIX: was `format!("user:{}", auth.key_id)` — use auth.actor() for
    // consistent audit record format across all handlers.
    spawn_audit_log(
        state.db.clone(),
        id,
        auth.actor(),
        AuditAction::WorkspaceReindexed,
        id,
        "workspace",
        Some(json!({ "enqueued": enqueued, "force": force })),
    );

    Ok(Json(ReindexResponse {
        enqueued,
        next_cursor,
    }))
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use axum::{
        body::{to_bytes, Body},
        http::{Method, Request, StatusCode},
    };
    use common::{
        config::AppConfig,
        models::{MemoryScope, MemoryType, MemoryUnit, ScopeVisibility},
        providers::{FastEmbedProvider, OllamaProvider},
    };
    use qdrant_client::Qdrant;
    use serde_json::Value;
    use sqlx::{types::Json, PgPool};
    use tokio::sync::Semaphore;
    use tower::ServiceExt;

    use super::*;

    async fn test_state(pool: PgPool) -> AppState {
        let redis_url =
            std::env::var("REDIS_URL").unwrap_or_else(|_| "redis://localhost:16379".to_owned());
        let redis = {
            let cfg = deadpool_redis::Config::from_url(&redis_url);
            match cfg.create_pool(Some(deadpool_redis::Runtime::Tokio1)) {
                Ok(pool) => pool,
                Err(error) => panic!("test Redis pool should be created: {error}"),
            }
        };
        let qdrant_url =
            std::env::var("QDRANT_URL").unwrap_or_else(|_| "http://localhost:16333".to_owned());
        let qdrant = match Qdrant::from_url(&qdrant_url).build() {
            Ok(client) => client,
            Err(error) => panic!("test Qdrant URL should be valid: {error}"),
        };
        let config = match AppConfig::from_toml_str(include_str!("../../../../config.toml")) {
            Ok(config) => config,
            Err(error) => panic!("checked-in config should parse: {error}"),
        };

        AppState {
            db: pool,
            redis,
            qdrant,
            processor_semaphore: Arc::new(Semaphore::new(
                usize::try_from(config.database.max_connections).unwrap_or(10),
            )),
            embedding_provider: Arc::new(FastEmbedProvider::new("test-embedding")),
            llm_provider: Arc::new(OllamaProvider::new(
                "http://127.0.0.1:9",
                "test-llm",
                1,
                None,
            )),
            config: Arc::new(config),
            github_webhook_secret: "test-secret".to_owned(),
        }
    }

    async fn insert_workspace(pool: &PgPool) -> Uuid {
        let workspace_id = Uuid::now_v7();
        let result = sqlx::query("INSERT INTO workspaces (id, name, config) VALUES ($1, $2, $3)")
            .bind(workspace_id)
            .bind(format!("workspace-{workspace_id}"))
            .bind(serde_json::json!({}))
            .execute(pool)
            .await;

        if let Err(error) = result {
            panic!("test workspace insert should succeed: {error}");
        }

        workspace_id
    }

    async fn insert_api_key(pool: &PgPool, workspace_id: Uuid) -> String {
        let key_id = Uuid::now_v7();
        let (plaintext, prefix) = crate::security::generate_api_key(workspace_id);
        let key_hash = match crate::security::hash_secret(&plaintext) {
            Ok(hash) => hash,
            Err(error) => panic!("test key hash should be generated: {error}"),
        };
        let result = sqlx::query(
            r#"
            INSERT INTO api_keys (
                id, workspace_id, name, key_hash, prefix, prefix_version, revoked, revoked_at
            )
            VALUES ($1, $2, $3, $4, $5, 2, false, NULL)
            "#,
        )
        .bind(key_id)
        .bind(workspace_id)
        .bind("test key")
        .bind(key_hash)
        .bind(prefix)
        .execute(pool)
        .await;

        if let Err(error) = result {
            panic!("test API key insert should succeed: {error}");
        }

        plaintext
    }

    async fn insert_memory(pool: &PgPool, workspace_id: Uuid, content: &str) -> Uuid {
        let memory_id = Uuid::now_v7();
        let result = sqlx::query(
            r#"
            INSERT INTO memory_units (
                id,
                workspace_id,
                scope,
                memory_type,
                content,
                entities,
                importance_score,
                tags
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            "#,
        )
        .bind(memory_id)
        .bind(workspace_id)
        .bind(serde_json::json!({
            "workspace_id": workspace_id,
            "agent_id": null,
            "user_id": null,
            "repo": null
        }))
        .bind(MemoryType::Episodic)
        .bind(content)
        .bind(serde_json::json!([]))
        .bind(0.8_f32)
        .bind(Vec::<String>::new())
        .execute(pool)
        .await;

        if let Err(error) = result {
            panic!("test memory insert should succeed: {error}");
        }

        memory_id
    }

    fn request(method: Method, uri: String, api_key: &str) -> Request<Body> {
        let builder = Request::builder()
            .method(method)
            .uri(uri)
            .header("content-type", "application/json")
            .header("x-api-key", api_key);

        match builder.body(Body::from(Value::Null.to_string())) {
            Ok(request) => request,
            Err(error) => panic!("test request should build: {error}"),
        }
    }

    fn request_with_body(
        method: Method,
        uri: String,
        api_key: &str,
        body: String,
    ) -> Request<Body> {
        let builder = Request::builder()
            .method(method)
            .uri(uri)
            .header("content-type", "application/x-ndjson")
            .header("x-api-key", api_key);

        match builder.body(Body::from(body)) {
            Ok(request) => request,
            Err(error) => panic!("test request should build: {error}"),
        }
    }

    async fn response_json(response: axum::response::Response) -> Value {
        let bytes = match to_bytes(response.into_body(), usize::MAX).await {
            Ok(bytes) => bytes,
            Err(error) => panic!("response body should be readable: {error}"),
        };
        match serde_json::from_slice::<Value>(&bytes) {
            Ok(value) => value,
            Err(error) => panic!("response body should be JSON: {error}"),
        }
    }

    fn history_series(body: &Value) -> &[Value] {
        match body.get("series").and_then(Value::as_array) {
            Some(series) => series.as_slice(),
            None => panic!("stats history response should include series array"),
        }
    }

    fn update_request(
        decay_half_life_days: Option<u32>,
        pruning_threshold: Option<f32>,
    ) -> UpdateWorkspaceConfigRequest {
        UpdateWorkspaceConfigRequest {
            promotion_threshold: None,
            dedup_cosine_threshold: None,
            llm_provider: None,
            llm_model: None,
            embedding_provider: None,
            embedding_model: None,
            decay_half_life_days,
            pruning_threshold,
            retention_max_age_days: None,
            compliance_hard_purge: None,
            contradiction_mode: None,
            contradiction_threshold: None,
            contradiction_candidates: None,
            sub_agent_pools: None,
            extra: serde_json::Map::new(),
        }
    }

    fn update_request_with_extra(
        key: &str,
        value: serde_json::Value,
    ) -> UpdateWorkspaceConfigRequest {
        let mut request = update_request(None, None);
        request.extra.insert(key.to_owned(), value);
        request
    }

    fn stats_row() -> WorkspaceStatsRow {
        WorkspaceStatsRow {
            total_memories: 0,
            episodic_count: 0,
            semantic_count: 0,
            pinned_count: 0,
            deleted_count: 0,
            avg_importance_score: None,
            avg_decay_score: None,
            memories_created_7d: 0,
            memories_created_30d: 0,
            oldest_memory_at: None,
            newest_memory_at: None,
        }
    }

    fn import_memory_unit(workspace_id: Uuid) -> MemoryUnit {
        let now = Utc::now();
        MemoryUnit {
            id: Uuid::now_v7(),
            workspace_id,
            scope: MemoryScope {
                workspace_id,
                agent_id: None,
                user_id: None,
                repo: Some("Quazmoz/memoryops".to_owned()),
            },
            memory_type: MemoryType::Episodic,
            scope_visibility: ScopeVisibility::Private,
            content: "imported memory".to_owned(),
            entities: Json(Vec::new()),
            importance_score: 0.8,
            importance_overridden: false,
            source_events: Vec::new(),
            embedding_id: Some("stale-embedding".to_owned()),
            token_count: Some(4),
            decay_score: 1.0,
            relevance_score: 0.5,
            pinned: false,
            tags: vec!["import".to_owned()],
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
    fn lifecycle_config_rejects_zero_half_life() {
        let error = match validate_lifecycle_config(&update_request(Some(0), None)) {
            Ok(()) => panic!("zero half-life should be rejected"),
            Err(error) => error,
        };

        assert!(
            matches!(error, AppError::Validation(message) if message == "decay_half_life_days must be between 1 and 3650")
        );
    }

    #[test]
    fn lifecycle_config_rejects_out_of_range_pruning_threshold() {
        let error = match validate_lifecycle_config(&update_request(None, Some(0.99))) {
            Ok(()) => panic!("high pruning threshold should be rejected"),
            Err(error) => error,
        };

        assert!(
            matches!(error, AppError::Validation(message) if message == "pruning_threshold must be between 0.01 and 0.50")
        );
    }

    #[test]
    fn lifecycle_config_accepts_valid_values() {
        assert!(validate_lifecycle_config(&update_request(Some(90), Some(0.15))).is_ok());
    }

    #[test]
    fn workspace_stats_coerces_null_averages_to_zero() {
        let stats = workspace_stats_from_row(stats_row());

        assert_eq!(stats.avg_importance_score, 0.0);
        assert_eq!(stats.avg_decay_score, 0.0);
    }

    #[test]
    fn workspace_stats_keeps_null_oldest_and_newest_as_none() {
        let stats = workspace_stats_from_row(stats_row());

        assert_eq!(stats.oldest_memory_at, None);
        assert_eq!(stats.newest_memory_at, None);
    }

    #[test]
    fn import_line_with_mismatched_workspace_id_is_skipped() {
        let target_workspace_id = Uuid::now_v7();
        let source_workspace_id = Uuid::now_v7();
        let memory = import_memory_unit(source_workspace_id);

        assert!(should_skip_import(&memory, target_workspace_id));
        assert!(!should_skip_import(&memory, source_workspace_id));
    }

    #[test]
    fn imported_memory_is_sanitized_for_target_workspace_and_reembedding() {
        let source_workspace_id = Uuid::now_v7();
        let target_workspace_id = Uuid::now_v7();
        let memory = import_memory_unit(source_workspace_id);

        let sanitized = sanitize_imported_memory(memory, target_workspace_id);

        assert_eq!(sanitized.workspace_id, target_workspace_id);
        assert_eq!(sanitized.scope.workspace_id, target_workspace_id);
        assert_eq!(sanitized.embedding_id, None);
        assert_eq!(sanitized.deleted_at, None);
        assert_eq!(sanitized.promoted_at, None);
        assert_eq!(sanitized.corroboration_count, 0);
        assert_eq!(sanitized.version, 1);
    }

    #[test]
    fn merge_workspace_config_rejects_known_key_in_extra_patch() {
        let mut target = serde_json::json!({});
        let patch = update_request_with_extra("pruning_threshold", serde_json::json!(0.3));

        let error = match merge_workspace_config(&mut target, &patch) {
            Ok(()) => panic!("known config key in extra should be rejected"),
            Err(error) => error,
        };

        assert!(
            matches!(error, AppError::Validation(message) if message.contains("must be set via the typed request field"))
        );
    }

    #[test]
    fn merge_workspace_config_applies_typed_model_provider_fields() {
        let mut target = serde_json::json!({});
        let mut patch = update_request(None, None);
        patch.llm_provider = Some("openai".to_owned());
        patch.llm_model = Some("gpt-4.1".to_owned());
        patch.embedding_provider = Some("openai".to_owned());
        patch.embedding_model = Some("text-embedding-3-large".to_owned());

        if let Err(error) = merge_workspace_config(&mut target, &patch) {
            panic!("typed model/provider fields should merge: {error}");
        }

        assert_eq!(target["llm_provider"], serde_json::json!("openai"));
        assert_eq!(target["llm_model"], serde_json::json!("gpt-4.1"));
        assert_eq!(
            target["embedding_provider"],
            serde_json::json!("openai")
        );
        assert_eq!(
            target["embedding_model"],
            serde_json::json!("text-embedding-3-large")
        );
    }

    #[test]
    fn merge_workspace_config_ignores_unknown_extra_patch_keys() {
        let mut target = serde_json::json!({});
        let patch = update_request_with_extra("totally_unknown_field", serde_json::json!("x"));

        if let Err(error) = merge_workspace_config(&mut target, &patch) {
            panic!("unknown config key should be ignored, not error: {error}");
        }

        assert!(
            target
                .as_object()
                .and_then(|obj| obj.get("totally_unknown_field"))
                .is_none(),
            "unknown key must not be merged into workspace config"
        );
    }

    #[test]
    fn stats_history_days_defaults_to_thirty() {
        let days = match stats_history_days(None) {
            Ok(days) => days,
            Err(error) => panic!("default stats history days should be valid: {error}"),
        };

        assert_eq!(days, 30);
    }

    #[test]
    fn stats_history_days_rejects_more_than_ninety() {
        let error = match stats_history_days(Some(91)) {
            Ok(_) => panic!("days above max should be rejected"),
            Err(error) => error,
        };

        assert!(
            matches!(error, AppError::Validation(message) if message == "days must be less than or equal to 90")
        );
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn stats_history_returns_empty_series_for_new_workspace(pool: PgPool) {
        let workspace_id = insert_workspace(&pool).await;
        let api_key = insert_api_key(&pool, workspace_id).await;
        let app = crate::router(test_state(pool).await);

        let response = match app
            .oneshot(request(
                Method::GET,
                format!("/v1/workspaces/{workspace_id}/stats/history"),
                &api_key,
            ))
            .await
        {
            Ok(response) => response,
            Err(error) => panic!("stats history request should respond: {error}"),
        };

        assert_eq!(response.status(), StatusCode::OK);
        let body = response_json(response).await;
        let series = history_series(&body);

        assert_eq!(series.len(), 30);
        assert!(series.iter().all(|point| {
            point.get("created").and_then(Value::as_i64) == Some(0)
                && point.get("promoted").and_then(Value::as_i64) == Some(0)
                && point.get("soft_deleted").and_then(Value::as_i64) == Some(0)
        }));
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn stats_history_respects_days_param(pool: PgPool) {
        let workspace_id = insert_workspace(&pool).await;
        let api_key = insert_api_key(&pool, workspace_id).await;
        let app = crate::router(test_state(pool).await);

        let response = match app
            .oneshot(request(
                Method::GET,
                format!("/v1/workspaces/{workspace_id}/stats/history?days=7"),
                &api_key,
            ))
            .await
        {
            Ok(response) => response,
            Err(error) => panic!("stats history request should respond: {error}"),
        };

        assert_eq!(response.status(), StatusCode::OK);
        let body = response_json(response).await;
        let series = history_series(&body);

        assert_eq!(series.len(), 7);
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn stats_history_rejects_days_above_ninety(pool: PgPool) {
        let workspace_id = insert_workspace(&pool).await;
        let api_key = insert_api_key(&pool, workspace_id).await;
        let app = crate::router(test_state(pool).await);

        let response = match app
            .oneshot(request(
                Method::GET,
                format!("/v1/workspaces/{workspace_id}/stats/history?days=91"),
                &api_key,
            ))
            .await
        {
            Ok(response) => response,
            Err(error) => panic!("stats history request should respond: {error}"),
        };

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn stats_history_rejects_days_zero(pool: PgPool) {
        let workspace_id = insert_workspace(&pool).await;
        let api_key = insert_api_key(&pool, workspace_id).await;
        let app = crate::router(test_state(pool).await);

        let response = match app
            .oneshot(request(
                Method::GET,
                format!("/v1/workspaces/{workspace_id}/stats/history?days=0"),
                &api_key,
            ))
            .await
        {
            Ok(response) => response,
            Err(error) => panic!("stats history request should respond: {error}"),
        };

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn stats_history_counts_created_memories(pool: PgPool) {
        let workspace_id = insert_workspace(&pool).await;
        let other_workspace_id = insert_workspace(&pool).await;
        let api_key = insert_api_key(&pool, workspace_id).await;
        insert_memory(&pool, workspace_id, "first workspace memory").await;
        insert_memory(&pool, workspace_id, "second workspace memory").await;
        insert_memory(&pool, other_workspace_id, "other workspace memory").await;
        let app = crate::router(test_state(pool).await);

        let response = match app
            .oneshot(request(
                Method::GET,
                format!("/v1/workspaces/{workspace_id}/stats/history?days=1"),
                &api_key,
            ))
            .await
        {
            Ok(response) => response,
            Err(error) => panic!("stats history request should respond: {error}"),
        };

        assert_eq!(response.status(), StatusCode::OK);
        let body = response_json(response).await;
        let series = history_series(&body);
        let today = match series.first() {
            Some(point) => point,
            None => panic!("one-day stats history should include one point"),
        };

        assert_eq!(series.len(), 1);
        assert_eq!(today.get("created").and_then(Value::as_i64), Some(2));
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn import_memories_returns_structured_422_when_body_exceeds_limit(pool: PgPool) {
        let workspace_id = insert_workspace(&pool).await;
        let api_key = insert_api_key(&pool, workspace_id).await;
        let app = crate::router(test_state(pool).await);

        let first_line = match serde_json::to_string(&import_memory_unit(workspace_id)) {
            Ok(line) => format!("{line}\n"),
            Err(error) => panic!("memory unit should serialize: {error}"),
        };
        let oversized_tail = "x".repeat(MAX_IMPORT_BODY_BYTES);
        let body = format!("{first_line}{oversized_tail}");

        let response = match app
            .oneshot(request_with_body(
                Method::POST,
                format!("/v1/workspaces/{workspace_id}/import"),
                &api_key,
                body,
            ))
            .await
        {
            Ok(response) => response,
            Err(error) => panic!("import request should respond: {error}"),
        };

        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        let payload = response_json(response).await;
        assert!(
            payload.get("errors").and_then(Value::as_u64).unwrap_or(0) >= 1,
            "errors should signal truncation"
        );
        assert!(payload.get("imported").is_some());
        assert!(payload.get("skipped").is_some());
    }
}
