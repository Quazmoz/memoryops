use anyhow::anyhow;
use axum::{extract::Path, extract::State, Extension, Json};
use common::{
    audit::spawn_audit_log,
    auth::AuthContext,
    error::AppResult,
    models::{AuditAction, Workspace, WorkspaceConfig},
    AppError, AppState,
};
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
    let config = serde_json::to_value(request.config.unwrap_or_default())
        .map_err(|error| AppError::Internal(anyhow!(error)))?;

    sqlx::query(
        r#"
        INSERT INTO workspaces (id, name, config)
        VALUES ($1, $2, $3)
        "#,
    )
    .bind(workspace_id)
    .bind(request.name.trim())
    .bind(config)
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
    Json(config): Json<WorkspaceConfig>,
) -> AppResult<Json<Workspace>> {
    require_workspace(&auth, id)?;
    let before = get_workspace_by_id(&state, id)
        .await?
        .ok_or_else(|| AppError::NotFound {
            resource: format!("workspace:{id}"),
        })?;
    let config_value =
        serde_json::to_value(config).map_err(|error| AppError::Internal(anyhow!(error)))?;
    let updated = sqlx::query_as::<_, Workspace>(
        r#"
        UPDATE workspaces
        SET config = $2
        WHERE id = $1 AND deleted_at IS NULL
        RETURNING id, name, config, created_at, updated_at, deleted_at
        "#,
    )
    .bind(id)
    .bind(config_value)
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

async fn get_workspace_by_id(state: &AppState, id: Uuid) -> AppResult<Option<Workspace>> {
    sqlx::query_as::<_, Workspace>(
        r#"
        SELECT id, name, config, created_at, updated_at, deleted_at
        FROM workspaces
        WHERE id = $1 AND deleted_at IS NULL
        "#,
    )
    .bind(id)
    .fetch_optional(&state.db)
    .await
    .map_err(AppError::Database)
}
