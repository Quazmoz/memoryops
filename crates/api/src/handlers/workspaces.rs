use std::{str, time::Duration};

use anyhow::anyhow;
use axum::{
    body::Body, extract::Path, extract::Query, extract::State, response::IntoResponse, Extension,
    Json,
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
use processor::promoter::{run_promotion_pass, PromoterConfig, PromotionReport};
use processor::worker::enqueue_slow_job;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::PgPool;
use uuid::Uuid;

use super::require_workspace;

pub const MAX_IMPORT_BODY_BYTES: usize = 50 * 1024 * 1024;

#[derive(Debug, Deserialize)]
pub struct CreateWorkspaceRequest {
    pub name: String,
    pub config: Option<WorkspaceConfig>,
}

#[derive(Debug, Serialize)]
pub struct CreateWorkspaceResponse {
    pub workspace_id: Uuid,
}

#[derive(Debug, Deserialize)]
pub struct UpdateWorkspaceConfigRequest {
    pub promotion_threshold: Option<f32>,
    pub dedup_cosine_threshold: Option<f32>,
    pub decay_half_life_days: Option<u32>,
    pub pruning_threshold: Option<f32>,
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

#[axum::debug_handler]
pub async fn create_workspace(
    State(state): State<AppState>,
    Json(request): Json<CreateWorkspaceRequest>,
) -> AppResult<Json<CreateWorkspaceResponse>> {
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
        INSERT INTO workspaces (id, name, config, promotion_threshold, dedup_cosine_threshold)
        VALUES ($1, $2, $3, $4, $5)
        "#,
    )
    .bind(workspace_id)
    .bind(request.name.trim())
    .bind(config_value)
    .bind(config.promotion_threshold)
    .bind(config.dedup_cosine_threshold)
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

    Ok(Json(CreateWorkspaceResponse { workspace_id }))
}

#[axum::debug_handler]
pub async fn list_workspaces(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthContext>,
) -> AppResult<Json<Vec<Workspace>>> {
    let workspace = get_workspace_by_id(&state, auth.workspace_id)
        .await?
        .ok_or_else(|| AppError::NotFound {
            resource: format!("workspace:{}", auth.workspace_id),
        })?;
    Ok(Json(vec![workspace]))
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
            SELECT DATE(updated_at AT TIME ZONE 'UTC') AS date, COUNT(*) AS promoted
            FROM memory_units
            WHERE workspace_id = $1
              AND memory_type = 'semantic'
              AND importance_score > 0
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
    .bind(auth.workspace_id)
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
    validate_contradiction_config(&config)?;
    validate_sub_agent_pools(&config)?;

    let before = get_workspace_by_id(&state, id)
        .await?
        .ok_or_else(|| AppError::NotFound {
            resource: format!("workspace:{id}"),
        })?;
    let mut config_value = before.config.clone();
    merge_workspace_config(&mut config_value, &config);
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
pub async fn promote(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthContext>,
    Path(id): Path<Uuid>,
) -> AppResult<Json<PromotionReport>> {
    require_workspace(&auth, id)?;
    let lock_key = format!("promotion:lock:{id}");
    acquire_promotion_lock(&state, &lock_key).await?;

    let config = fetch_workspace_promotion_config(&state, id).await?;
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

    release_promotion_lock(&state, &lock_key).await;

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

    Ok(Json(response))
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
            let mut redis = state.redis.clone();
            if let Err(error) = enqueue_slow_job(&mut redis, memory_id, workspace_id, 0).await {
                tracing::warn!(error = ?error, memory_id = %memory_id, "failed to enqueue imported memory for embedding");
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

async fn fetch_workspace_promotion_config(
    state: &AppState,
    workspace_id: Uuid,
) -> AppResult<PromoterConfig> {
    #[derive(Debug, sqlx::FromRow)]
    struct Row {
        promotion_threshold: f64,
        dedup_cosine_threshold: f64,
    }

    let row = sqlx::query_as::<_, Row>(
        r#"
        SELECT promotion_threshold, dedup_cosine_threshold
        FROM workspaces
        WHERE id = $1 AND deleted_at IS NULL
        "#,
    )
    .bind(workspace_id)
    .fetch_optional(&state.db)
    .await
    .map_err(AppError::Database)?
    .ok_or_else(|| AppError::NotFound {
        resource: format!("workspace:{workspace_id}"),
    })?;

    Ok(PromoterConfig {
        promotion_threshold: row.promotion_threshold as f32,
        dedup_cosine_threshold: row.dedup_cosine_threshold as f32,
        cluster_min_size: 3,
        batch_size: 200,
    })
}

async fn acquire_promotion_lock(state: &AppState, key: &str) -> AppResult<()> {
    let mut redis = state.redis.clone();
    let acquired = redis::cmd("SET")
        .arg(key)
        .arg("1")
        .arg("NX")
        .arg("EX")
        .arg(600)
        .query_async::<Option<String>>(&mut redis)
        .await
        .map_err(|error| AppError::Internal(anyhow!(error)))?
        .is_some();

    if acquired {
        Ok(())
    } else {
        Err(AppError::Conflict(
            "promotion already running for this workspace".to_owned(),
        ))
    }
}

async fn release_promotion_lock(state: &AppState, key: &str) {
    let mut redis = state.redis.clone();
    if let Err(error) = redis::cmd("DEL")
        .arg(key)
        .query_async::<i64>(&mut redis)
        .await
    {
        tracing::warn!(error = ?error, key, "failed to release promotion lock");
    }
}

fn merge_workspace_config(target: &mut serde_json::Value, patch: &UpdateWorkspaceConfigRequest) {
    if !target.is_object() {
        *target = json!({});
    }

    let Some(object) = target.as_object_mut() else {
        return;
    };

    if let Some(value) = patch.promotion_threshold {
        object.insert("promotion_threshold".to_owned(), json!(value));
    }
    if let Some(value) = patch.dedup_cosine_threshold {
        object.insert("dedup_cosine_threshold".to_owned(), json!(value));
    }
    if let Some(value) = patch.decay_half_life_days {
        object.insert("decay_half_life_days".to_owned(), json!(value));
    }
    if let Some(value) = patch.pruning_threshold {
        object.insert("pruning_threshold".to_owned(), json!(value));
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
    for (key, value) in &patch.extra {
        object.insert(key.clone(), value.clone());
    }
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
    use redis::aio::ConnectionManager;
    use serde_json::Value;
    use sqlx::{types::Json, PgPool};
    use tower::ServiceExt;

    use super::*;

    async fn test_state(pool: PgPool) -> AppState {
        let redis_url =
            std::env::var("REDIS_URL").unwrap_or_else(|_| "redis://localhost:16379".to_owned());
        let redis_client = match redis::Client::open(redis_url) {
            Ok(client) => client,
            Err(error) => panic!("test Redis URL should be valid: {error}"),
        };
        let redis = match ConnectionManager::new(redis_client).await {
            Ok(connection) => connection,
            Err(error) => panic!("test Redis should be reachable: {error}"),
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
            embedding_provider: Arc::new(FastEmbedProvider::new("test-embedding")),
            llm_provider: Arc::new(OllamaProvider::new("http://127.0.0.1:9", "test-llm", 1)),
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
            INSERT INTO api_keys (id, workspace_id, name, key_hash, prefix, revoked, revoked_at)
            VALUES ($1, $2, $3, $4, $5, false, NULL)
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
            decay_half_life_days,
            pruning_threshold,
            contradiction_mode: None,
            contradiction_threshold: None,
            contradiction_candidates: None,
            sub_agent_pools: None,
            extra: serde_json::Map::new(),
        }
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
    #[ignore = "requires live PostgreSQL and Redis from docker-compose.test.yml"]
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
    #[ignore = "requires live PostgreSQL and Redis from docker-compose.test.yml"]
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
    #[ignore = "requires live PostgreSQL and Redis from docker-compose.test.yml"]
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
    #[ignore = "requires live PostgreSQL and Redis from docker-compose.test.yml"]
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
    #[ignore = "requires live PostgreSQL and Redis from docker-compose.test.yml"]
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
}
