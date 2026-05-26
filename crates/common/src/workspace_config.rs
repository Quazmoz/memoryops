use uuid::Uuid;

use crate::{
    error::AppResult,
    models::{WorkspaceConfig, DEFAULT_DECAY_HALF_LIFE_DAYS},
    AppError,
};

pub async fn load_workspace_config(
    db: &sqlx::PgPool,
    workspace_id: Uuid,
) -> AppResult<WorkspaceConfig> {
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

    match serde_json::from_value::<WorkspaceConfig>(value) {
        Ok(config) => Ok(config),
        Err(error) => {
            tracing::warn!(
                workspace_id = %workspace_id,
                error = ?error,
                "failed to parse workspace config; using defaults"
            );
            Ok(WorkspaceConfig::default())
        }
    }
}

pub async fn load_workspace_half_life_days(
    db: &sqlx::PgPool,
    workspace_id: Uuid,
) -> AppResult<f64> {
    let config = load_workspace_config(db, workspace_id).await?;
    let half_life_days = config
        .decay_half_life_days
        .map(f64::from)
        .unwrap_or(f64::from(DEFAULT_DECAY_HALF_LIFE_DAYS));

    if half_life_days > 0.0 {
        Ok(half_life_days)
    } else {
        Ok(f64::from(DEFAULT_DECAY_HALF_LIFE_DAYS))
    }
}
