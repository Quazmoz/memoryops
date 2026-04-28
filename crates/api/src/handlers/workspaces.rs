use std::time::Duration;

use anyhow::anyhow;
use axum::{extract::Path, extract::State, Extension, Json};
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
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
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
        RETURNING id, name, config, promotion_threshold, dedup_cosine_threshold, created_at, updated_at, deleted_at
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
        SELECT id, name, config, promotion_threshold, dedup_cosine_threshold, created_at, updated_at, deleted_at
        FROM workspaces
        WHERE id = $1 AND deleted_at IS NULL
        "#,
    )
    .bind(id)
    .fetch_optional(&state.db)
    .await
    .map_err(AppError::Database)
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
    for (key, value) in &patch.extra {
        object.insert(key.clone(), value.clone());
    }
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
