use anyhow::anyhow;
use axum::{extract::Path, extract::State, Extension, Json};
use common::{
    audit::spawn_audit_log,
    auth::AuthContext,
    error::AppResult,
    models::AuditAction,
    services::WorkspaceConfigService,
    AppError, AppState,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use uuid::Uuid;

use super::require_workspace;

#[derive(Debug, Deserialize)]
pub struct ForgetUserPath {
    pub workspace_id: Uuid,
    pub user_id: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ForgetUserResponse {
    pub user_id: String,
    pub memories_purged: u64,
    pub raw_events_purged: u64,
    pub mode: String,
}

#[axum::debug_handler]
pub async fn forget_user_data(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthContext>,
    Path(path): Path<ForgetUserPath>,
) -> AppResult<Json<ForgetUserResponse>> {
    require_workspace(&auth, path.workspace_id)?;

    if path.user_id.trim().is_empty() {
        return Err(AppError::Validation("user_id is required".to_owned()));
    }

    let config = WorkspaceConfigService::new(state.db.clone())
        .load(path.workspace_id)
        .await?;

    let (memories_purged, raw_events_purged, mode) = if config.compliance_hard_purge {
        let mut tx = state.db.begin().await.map_err(AppError::Database)?;
        let source_event_ids =
            user_source_event_ids(&mut tx, path.workspace_id, &path.user_id).await?;
        let raw_events_purged =
            delete_raw_events(&mut tx, path.workspace_id, &source_event_ids).await?;
        let memories_purged =
            hard_delete_user_memories(&mut tx, path.workspace_id, &path.user_id).await?;
        tx.commit().await.map_err(AppError::Database)?;
        (memories_purged, raw_events_purged, "hard_purge")
    } else {
        let mut tx = state.db.begin().await.map_err(AppError::Database)?;
        let memories_purged =
            soft_delete_user_memories(&mut tx, path.workspace_id, &path.user_id).await?;
        tx.commit().await.map_err(AppError::Database)?;
        (memories_purged, 0, "soft_delete")
    };

    insert_compliance_audit_log(
        &state,
        path.workspace_id,
        &path.user_id,
        memories_purged,
        raw_events_purged,
        &auth.key_prefix,
    )
    .await?;

    spawn_audit_log(
        state.db.clone(),
        path.workspace_id,
        auth.actor(),
        AuditAction::UserErasure,
        path.workspace_id,
        "workspace",
        Some(json!({
            "user_id": path.user_id.clone(),
            "memories_purged": memories_purged,
            "raw_events_purged": raw_events_purged
        })),
    );

    Ok(Json(ForgetUserResponse {
        user_id: path.user_id,
        memories_purged,
        raw_events_purged,
        mode: mode.to_owned(),
    }))
}

async fn user_source_event_ids(
    conn: &mut sqlx::PgConnection,
    workspace_id: Uuid,
    user_id: &str,
) -> AppResult<Vec<Uuid>> {
    let source_event_ids = sqlx::query_scalar::<_, Option<Vec<Uuid>>>(
        r#"
        SELECT ARRAY_AGG(DISTINCT source_event_id)
               FILTER (WHERE source_event_id IS NOT NULL)
        FROM (
            SELECT UNNEST(source_events) AS source_event_id
            FROM memory_units
            WHERE workspace_id = $1
              AND scope->>'user_id' = $2
              AND hard_deleted_at IS NULL
        ) AS source_events
        "#,
    )
    .bind(workspace_id)
    .bind(user_id)
    .fetch_one(conn)
    .await
    .map_err(AppError::Database)?;

    Ok(source_event_ids.unwrap_or_default())
}

async fn delete_raw_events(
    conn: &mut sqlx::PgConnection,
    workspace_id: Uuid,
    source_event_ids: &[Uuid],
) -> AppResult<u64> {
    if source_event_ids.is_empty() {
        return Ok(0);
    }

    sqlx::query(
        r#"
        DELETE FROM raw_events
        WHERE workspace_id = $1 AND id = ANY($2)
        "#,
    )
    .bind(workspace_id)
    .bind(source_event_ids)
    .execute(conn)
    .await
    .map(|result| result.rows_affected())
    .map_err(AppError::Database)
}

async fn hard_delete_user_memories(
    conn: &mut sqlx::PgConnection,
    workspace_id: Uuid,
    user_id: &str,
) -> AppResult<u64> {
    sqlx::query(
        r#"
        DELETE FROM memory_units
        WHERE workspace_id = $1
          AND scope->>'user_id' = $2
        "#,
    )
    .bind(workspace_id)
    .bind(user_id)
    .execute(conn)
    .await
    .map(|result| result.rows_affected())
    .map_err(AppError::Database)
}

async fn soft_delete_user_memories(
    conn: &mut sqlx::PgConnection,
    workspace_id: Uuid,
    user_id: &str,
) -> AppResult<u64> {
    sqlx::query(
        r#"
        UPDATE memory_units
        SET deleted_at = NOW(), updated_at = NOW()
        WHERE workspace_id = $1
          AND scope->>'user_id' = $2
          AND deleted_at IS NULL
        "#,
    )
    .bind(workspace_id)
    .bind(user_id)
    .execute(conn)
    .await
    .map(|result| result.rows_affected())
    .map_err(AppError::Database)
}

async fn insert_compliance_audit_log(
    state: &AppState,
    workspace_id: Uuid,
    user_id: &str,
    memories_purged: u64,
    raw_events_purged: u64,
    initiated_by: &str,
) -> AppResult<()> {
    sqlx::query(
        r#"
        INSERT INTO compliance_audit_log (
            workspace_id,
            action,
            target_user_id,
            memories_purged,
            raw_events_purged,
            initiated_by
        )
        VALUES ($1, 'user_erasure', $2, $3, $4, $5)
        "#,
    )
    .bind(workspace_id)
    .bind(user_id)
    .bind(rows_to_i32(memories_purged)?)
    .bind(rows_to_i32(raw_events_purged)?)
    .bind(initiated_by)
    .execute(&state.db)
    .await
    .map(|_| ())
    .map_err(AppError::Database)
}

fn rows_to_i32(value: u64) -> AppResult<i32> {
    i32::try_from(value).map_err(|error| AppError::Internal(anyhow!(error)))
}
