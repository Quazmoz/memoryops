use std::time::Duration;

use anyhow::anyhow;
use axum::{extract::Path, extract::Query, extract::State, Extension, Json};
use chrono::{DateTime, NaiveDate, Utc};
use common::{
    audit::spawn_audit_log,
    auth::AuthContext,
    error::AppResult,
    models::{AuditAction, Workspace, WorkspaceConfig},
    AppError, AppState,
};
use processor::promoter::{run_promotion_pass, PromoterConfig, PromotionReport};
use serde::{Deserialize, Serialize};
use serde_json::json;
use uuid::Uuid;

use super::require_workspace;

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
    .map_err(AppError::Database)?;

    Ok(Json(CreateWorkspaceResponse { workspace_id }))
}

#[axum::debug_handler]
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
    for (key, value) in &patch.extra {
        object.insert(key.clone(), value.clone());
    }
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
    use super::*;

    fn update_request(
        decay_half_life_days: Option<u32>,
        pruning_threshold: Option<f32>,
    ) -> UpdateWorkspaceConfigRequest {
        UpdateWorkspaceConfigRequest {
            promotion_threshold: None,
            dedup_cosine_threshold: None,
            decay_half_life_days,
            pruning_threshold,
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
    fn stats_history_days_defaults_to_thirty() {
        assert_eq!(stats_history_days(None).expect("default days"), 30);
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
}
