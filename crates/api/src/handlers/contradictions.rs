use axum::{extract::Path, extract::Query, extract::State, Extension, Json};
use chrono::{DateTime, Utc};
use common::{
    audit::spawn_audit_log, auth::AuthContext, error::AppResult, models::AuditAction, AppError,
    AppState,
};
use processor::embedder::Embedder;
use retrieval::store::soft_delete_memory_unit;
use serde::{Deserialize, Serialize};
use serde_json::json;
use uuid::Uuid;

use super::require_workspace;

const DEFAULT_LIMIT: i64 = 20;
const MAX_LIMIT: i64 = 100;
const BULK_DISMISS_MAX: usize = 50;

#[derive(Debug, Deserialize)]
pub struct ContradictionQuery {
    pub status: Option<String>,
    pub after: Option<Uuid>,
    pub limit: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct ResolveContradictionRequest {
    pub resolution: String,
    pub notes: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct BulkDismissRequest {
    pub flag_ids: Vec<Uuid>,
    pub notes: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct BulkDismissResponse {
    pub dismissed: usize,
}

#[derive(Debug, Serialize)]
pub struct ContradictionListResponse {
    pub items: Vec<ContradictionItem>,
    pub next_cursor: Option<Uuid>,
}

#[derive(Debug, Serialize)]
pub struct ContradictionItem {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub memory_a: ContradictionMemoryRef,
    pub memory_b: ContradictionMemoryRef,
    pub similarity: f32,
    pub conflict_score: f32,
    pub resolution: String,
    pub resolved_by: Option<String>,
    pub resolved_at: Option<DateTime<Utc>>,
    pub notes: Option<String>,
    pub kept_memory_id: Option<Uuid>,
    pub discarded_memory_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
pub struct ContradictionMemoryRef {
    pub id: Uuid,
    pub content_preview: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
pub struct ContradictionCountResponse {
    pub open: i64,
}

#[derive(Debug, sqlx::FromRow)]
struct ContradictionRow {
    id: Uuid,
    workspace_id: Uuid,
    memory_a_id: Uuid,
    memory_a_content_preview: String,
    memory_a_created_at: DateTime<Utc>,
    memory_b_id: Uuid,
    memory_b_content_preview: String,
    memory_b_created_at: DateTime<Utc>,
    similarity: f32,
    conflict_score: f32,
    resolution: String,
    resolved_by: Option<String>,
    resolved_at: Option<DateTime<Utc>>,
    notes: Option<String>,
    kept_memory_id: Option<Uuid>,
    discarded_memory_id: Option<Uuid>,
    created_at: DateTime<Utc>,
}

impl From<ContradictionRow> for ContradictionItem {
    fn from(row: ContradictionRow) -> Self {
        Self {
            id: row.id,
            workspace_id: row.workspace_id,
            memory_a: ContradictionMemoryRef {
                id: row.memory_a_id,
                content_preview: row.memory_a_content_preview,
                created_at: row.memory_a_created_at,
            },
            memory_b: ContradictionMemoryRef {
                id: row.memory_b_id,
                content_preview: row.memory_b_content_preview,
                created_at: row.memory_b_created_at,
            },
            similarity: row.similarity,
            conflict_score: row.conflict_score,
            resolution: row.resolution,
            resolved_by: row.resolved_by,
            resolved_at: row.resolved_at,
            notes: row.notes,
            kept_memory_id: row.kept_memory_id,
            discarded_memory_id: row.discarded_memory_id,
            created_at: row.created_at,
        }
    }
}

#[axum::debug_handler]
pub async fn list_contradictions(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthContext>,
    Path(id): Path<Uuid>,
    Query(query): Query<ContradictionQuery>,
) -> AppResult<Json<ContradictionListResponse>> {
    require_workspace(&auth, id)?;
    let status = normalize_status(query.status.as_deref())?.unwrap_or("open");
    let limit = query.limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT);
    let mut rows = sqlx::query_as::<_, ContradictionRow>(
        r#"
        WITH cursor_row AS (
            SELECT created_at
            FROM contradiction_flags
            WHERE workspace_id = $1 AND id = $3
        )
        SELECT f.id,
               f.workspace_id,
               a.id AS memory_a_id,
               LEFT(a.content, 200) AS memory_a_content_preview,
               a.created_at AS memory_a_created_at,
               b.id AS memory_b_id,
               LEFT(b.content, 200) AS memory_b_content_preview,
               b.created_at AS memory_b_created_at,
               f.similarity::REAL AS similarity,
               f.conflict_score::REAL AS conflict_score,
               f.resolution::TEXT AS resolution,
               f.resolved_by,
               f.resolved_at,
               f.notes,
               f.kept_memory_id,
               f.discarded_memory_id,
               f.created_at
        FROM contradiction_flags f
        JOIN memory_units a ON a.id = f.memory_id_a AND a.workspace_id = f.workspace_id
        JOIN memory_units b ON b.id = f.memory_id_b AND b.workspace_id = f.workspace_id
        WHERE f.workspace_id = $1
          AND f.resolution = $2::contradiction_resolution
          AND ($3::UUID IS NULL OR f.created_at < (SELECT created_at FROM cursor_row))
        ORDER BY f.created_at DESC, f.id DESC
        LIMIT $4
        "#,
    )
    .bind(id)
    .bind(status)
    .bind(query.after)
    .bind(limit + 1)
    .fetch_all(&state.db)
    .await
    .map_err(AppError::Database)?;
    let next_cursor = if rows.len() > limit as usize {
        rows.pop().map(|row| row.id)
    } else {
        None
    };

    Ok(Json(ContradictionListResponse {
        items: rows.into_iter().map(ContradictionItem::from).collect(),
        next_cursor,
    }))
}

#[axum::debug_handler]
pub async fn resolve_contradiction(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthContext>,
    Path((id, flag_id)): Path<(Uuid, Uuid)>,
    Json(request): Json<ResolveContradictionRequest>,
) -> AppResult<Json<ContradictionItem>> {
    require_workspace(&auth, id)?;
    let resolution = normalize_resolution(&request.resolution)?;
    let notes = request
        .notes
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());

    let is_keep = resolution == "keep_a" || resolution == "keep_b";

    if is_keep {
        resolve_keep(&state, &auth, id, flag_id, resolution, notes).await
    } else {
        resolve_simple(&state, &auth, id, flag_id, resolution, notes).await
    }
}

async fn resolve_simple(
    state: &AppState,
    auth: &AuthContext,
    workspace_id: Uuid,
    flag_id: Uuid,
    resolution: &str,
    notes: Option<&str>,
) -> AppResult<Json<ContradictionItem>> {
    let row = sqlx::query_as::<_, ContradictionRow>(
        r#"
        WITH updated AS (
            UPDATE contradiction_flags
            SET resolution = $3::contradiction_resolution,
                notes = $4,
                resolved_by = $5,
                resolved_at = now()
            WHERE workspace_id = $1 AND id = $2
            RETURNING *
        )
        SELECT f.id,
               f.workspace_id,
               a.id AS memory_a_id,
               LEFT(a.content, 200) AS memory_a_content_preview,
               a.created_at AS memory_a_created_at,
               b.id AS memory_b_id,
               LEFT(b.content, 200) AS memory_b_content_preview,
               b.created_at AS memory_b_created_at,
               f.similarity::REAL AS similarity,
               f.conflict_score::REAL AS conflict_score,
               f.resolution::TEXT AS resolution,
               f.resolved_by,
               f.resolved_at,
               f.notes,
               f.kept_memory_id,
               f.discarded_memory_id,
               f.created_at
        FROM updated f
        JOIN memory_units a ON a.id = f.memory_id_a AND a.workspace_id = f.workspace_id
        JOIN memory_units b ON b.id = f.memory_id_b AND b.workspace_id = f.workspace_id
        "#,
    )
    .bind(workspace_id)
    .bind(flag_id)
    .bind(resolution)
    .bind(notes)
    .bind(format!("user:{}", auth.key_id))
    .fetch_optional(&state.db)
    .await
    .map_err(AppError::Database)?
    .ok_or_else(|| AppError::NotFound {
        resource: format!("contradiction_flag:{flag_id}"),
    })?;

    Ok(Json(ContradictionItem::from(row)))
}

async fn resolve_keep(
    state: &AppState,
    auth: &AuthContext,
    workspace_id: Uuid,
    flag_id: Uuid,
    resolution: &str,
    notes: Option<&str>,
) -> AppResult<Json<ContradictionItem>> {
    #[derive(Debug, sqlx::FromRow)]
    struct FlagIds {
        memory_id_a: Uuid,
        memory_id_b: Uuid,
    }

    let ids = sqlx::query_as::<_, FlagIds>(
        "SELECT memory_id_a, memory_id_b FROM contradiction_flags WHERE workspace_id = $1 AND id = $2",
    )
    .bind(workspace_id)
    .bind(flag_id)
    .fetch_optional(&state.db)
    .await
    .map_err(AppError::Database)?
    .ok_or_else(|| AppError::NotFound {
        resource: format!("contradiction_flag:{flag_id}"),
    })?;

    let (winner_id, loser_id) = if resolution == "keep_a" {
        (ids.memory_id_a, ids.memory_id_b)
    } else {
        (ids.memory_id_b, ids.memory_id_a)
    };

    // Verify loser is not already deleted
    let loser_alive = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM memory_units WHERE workspace_id = $1 AND id = $2 AND deleted_at IS NULL)",
    )
    .bind(workspace_id)
    .bind(loser_id)
    .fetch_one(&state.db)
    .await
    .map_err(AppError::Database)?;

    if !loser_alive {
        return Err(AppError::Validation(
            "the memory to be discarded is already archived".to_owned(),
        ));
    }

    // Soft-delete the loser
    soft_delete_memory_unit(&state.db, loser_id, workspace_id).await?;

    // Remove the loser's Qdrant point (best-effort)
    let embedder = Embedder::from_state(state);
    if let Err(error) = embedder.delete_point(loser_id).await {
        tracing::warn!(error = ?error, memory_id = %loser_id, "failed to delete Qdrant point for discarded memory");
    }

    // Update the flag: set resolution + kept/discarded ids
    let row = sqlx::query_as::<_, ContradictionRow>(
        r#"
        WITH updated AS (
            UPDATE contradiction_flags
            SET resolution = $3::contradiction_resolution,
                notes = $4,
                resolved_by = $5,
                resolved_at = now(),
                kept_memory_id = $6,
                discarded_memory_id = $7
            WHERE workspace_id = $1 AND id = $2
            RETURNING *
        )
        SELECT f.id,
               f.workspace_id,
               a.id AS memory_a_id,
               LEFT(a.content, 200) AS memory_a_content_preview,
               a.created_at AS memory_a_created_at,
               b.id AS memory_b_id,
               LEFT(b.content, 200) AS memory_b_content_preview,
               b.created_at AS memory_b_created_at,
               f.similarity::REAL AS similarity,
               f.conflict_score::REAL AS conflict_score,
               f.resolution::TEXT AS resolution,
               f.resolved_by,
               f.resolved_at,
               f.notes,
               f.kept_memory_id,
               f.discarded_memory_id,
               f.created_at
        FROM updated f
        JOIN memory_units a ON a.id = f.memory_id_a AND a.workspace_id = f.workspace_id
        JOIN memory_units b ON b.id = f.memory_id_b AND b.workspace_id = f.workspace_id
        "#,
    )
    .bind(workspace_id)
    .bind(flag_id)
    .bind(resolution)
    .bind(notes)
    .bind(format!("user:{}", auth.key_id))
    .bind(winner_id)
    .bind(loser_id)
    .fetch_optional(&state.db)
    .await
    .map_err(AppError::Database)?
    .ok_or_else(|| AppError::NotFound {
        resource: format!("contradiction_flag:{flag_id}"),
    })?;

    spawn_audit_log(
        state.db.clone(),
        workspace_id,
        format!("user:{}", auth.key_id),
        AuditAction::ContradictionResolved,
        flag_id,
        "contradiction_flag",
        Some(json!({
            "resolution": resolution,
            "kept": winner_id,
            "discarded": loser_id,
        })),
    );

    Ok(Json(ContradictionItem::from(row)))
}

#[axum::debug_handler]
pub async fn bulk_dismiss_contradictions(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthContext>,
    Path(id): Path<Uuid>,
    Json(request): Json<BulkDismissRequest>,
) -> AppResult<Json<BulkDismissResponse>> {
    require_workspace(&auth, id)?;

    if request.flag_ids.is_empty() {
        return Ok(Json(BulkDismissResponse { dismissed: 0 }));
    }
    if request.flag_ids.len() > BULK_DISMISS_MAX {
        return Err(AppError::Validation(format!(
            "bulk dismiss accepts at most {BULK_DISMISS_MAX} flag IDs"
        )));
    }

    let notes = request
        .notes
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());

    let dismissed = sqlx::query_scalar::<_, i64>(
        r#"
        WITH updated AS (
            UPDATE contradiction_flags
            SET resolution = 'dismissed'::contradiction_resolution,
                resolved_by = $3,
                resolved_at = now(),
                notes = COALESCE($4, notes)
            WHERE workspace_id = $1
              AND id = ANY($2)
              AND resolution = 'open'
            RETURNING id
        )
        SELECT COUNT(*) FROM updated
        "#,
    )
    .bind(id)
    .bind(&request.flag_ids)
    .bind(format!("user:{}", auth.key_id))
    .bind(notes)
    .fetch_one(&state.db)
    .await
    .map_err(AppError::Database)?;

    let dismissed_count = usize::try_from(dismissed).unwrap_or(0);

    if dismissed_count > 0 {
        spawn_audit_log(
            state.db.clone(),
            id,
            format!("user:{}", auth.key_id),
            AuditAction::ContradictionResolved,
            id,
            "contradiction_flags_bulk",
            Some(json!({
                "dismissed": dismissed_count,
                "flag_ids": request.flag_ids,
            })),
        );
    }

    Ok(Json(BulkDismissResponse {
        dismissed: dismissed_count,
    }))
}

#[axum::debug_handler]
pub async fn get_contradiction_count(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthContext>,
    Path(id): Path<Uuid>,
) -> AppResult<Json<ContradictionCountResponse>> {
    require_workspace(&auth, id)?;
    let open = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM contradiction_flags WHERE workspace_id = $1 AND resolution = 'open'",
    )
    .bind(id)
    .fetch_one(&state.db)
    .await
    .map_err(AppError::Database)?;

    Ok(Json(ContradictionCountResponse { open }))
}

fn normalize_status(status: Option<&str>) -> AppResult<Option<&'static str>> {
    status.map(normalize_resolution_status).transpose()
}

fn normalize_resolution_status(status: &str) -> AppResult<&'static str> {
    match status {
        "open" => Ok("open"),
        "auto_resolved" => Ok("auto_resolved"),
        "dismissed" => Ok("dismissed"),
        "accepted" => Ok("accepted"),
        "keep_a" => Ok("keep_a"),
        "keep_b" => Ok("keep_b"),
        _ => Err(AppError::Validation(
            "status must be one of: open, auto_resolved, dismissed, accepted, keep_a, keep_b"
                .to_owned(),
        )),
    }
}

fn normalize_resolution(resolution: &str) -> AppResult<&'static str> {
    match resolution {
        "accepted" => Ok("accepted"),
        "dismissed" => Ok("dismissed"),
        "keep_a" => Ok("keep_a"),
        "keep_b" => Ok("keep_b"),
        _ => Err(AppError::Validation(
            "resolution must be one of: accepted, dismissed, keep_a, keep_b".to_owned(),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_resolution_accepts_all_valid_values() {
        assert_eq!(normalize_resolution("accepted").ok(), Some("accepted"));
        assert_eq!(normalize_resolution("dismissed").ok(), Some("dismissed"));
        assert_eq!(normalize_resolution("keep_a").ok(), Some("keep_a"));
        assert_eq!(normalize_resolution("keep_b").ok(), Some("keep_b"));
    }

    #[test]
    fn normalize_resolution_rejects_unknown_value() {
        assert!(normalize_resolution("open").is_err());
        assert!(normalize_resolution("auto_resolved").is_err());
        assert!(normalize_resolution("bogus").is_err());
    }

    #[test]
    fn normalize_status_accepts_all_tab_values() {
        for status in [
            "open",
            "auto_resolved",
            "dismissed",
            "accepted",
            "keep_a",
            "keep_b",
        ] {
            assert!(
                normalize_status(Some(status)).is_ok(),
                "expected {status} to be accepted"
            );
        }
    }
}
