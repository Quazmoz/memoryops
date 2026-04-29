use axum::{extract::Path, extract::Query, extract::State, Extension, Json};
use chrono::{DateTime, Utc};
use common::{auth::AuthContext, error::AppResult, AppError, AppState};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::require_workspace;

const DEFAULT_LIMIT: i64 = 20;
const MAX_LIMIT: i64 = 100;

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
               f.created_at
        FROM updated f
        JOIN memory_units a ON a.id = f.memory_id_a AND a.workspace_id = f.workspace_id
        JOIN memory_units b ON b.id = f.memory_id_b AND b.workspace_id = f.workspace_id
        "#,
    )
    .bind(id)
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
        _ => Err(AppError::Validation(
            "status must be one of: open, auto_resolved, dismissed, accepted".to_owned(),
        )),
    }
}

fn normalize_resolution(resolution: &str) -> AppResult<&'static str> {
    match resolution {
        "accepted" => Ok("accepted"),
        "dismissed" => Ok("dismissed"),
        _ => Err(AppError::Validation(
            "resolution must be accepted or dismissed".to_owned(),
        )),
    }
}
