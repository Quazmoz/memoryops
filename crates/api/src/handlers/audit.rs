use axum::{extract::Path, extract::Query, extract::State, Extension, Json};
use chrono::{DateTime, Utc};
use common::{auth::AuthContext, error::AppResult, models::AuditEntry, AppError, AppState};
use serde::{Deserialize, Serialize};
use sqlx::{Postgres, QueryBuilder};
use uuid::Uuid;

use super::require_workspace;

const DEFAULT_AUDIT_LIMIT: i64 = 20;
const MAX_AUDIT_LIMIT: i64 = 100;

#[derive(Debug, Deserialize)]
pub struct AuditQuery {
    pub limit: Option<i64>,
    pub offset: Option<i64>,
    pub actor: Option<String>,
    pub action: Option<String>,
    pub since: Option<DateTime<Utc>>,
}

#[derive(Debug, Serialize)]
pub struct AuditResponse {
    pub items: Vec<AuditEntry>,
    pub limit: i64,
    pub offset: i64,
}

#[axum::debug_handler]
pub async fn list_audit(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthContext>,
    Path(id): Path<Uuid>,
    Query(query): Query<AuditQuery>,
) -> AppResult<Json<AuditResponse>> {
    require_workspace(&auth, id)?;
    let limit = query
        .limit
        .unwrap_or(DEFAULT_AUDIT_LIMIT)
        .clamp(1, MAX_AUDIT_LIMIT);
    let offset = query.offset.unwrap_or(0).max(0);

    let mut builder = QueryBuilder::<Postgres>::new(
        "SELECT id, workspace_id, actor, action, target_id, target_type, diff, occurred_at FROM audit_log WHERE workspace_id = ",
    );
    builder.push_bind(id);

    if let Some(actor) = &query.actor {
        builder.push(" AND actor = ");
        builder.push_bind(actor);
    }
    if let Some(action) = &query.action {
        builder.push(" AND action::text = ");
        builder.push_bind(action);
    }
    if let Some(since) = query.since {
        builder.push(" AND occurred_at >= ");
        builder.push_bind(since);
    }

    builder.push(" ORDER BY occurred_at DESC LIMIT ");
    builder.push_bind(limit);
    builder.push(" OFFSET ");
    builder.push_bind(offset);

    let items = builder
        .build_query_as::<AuditEntry>()
        .fetch_all(&state.db)
        .await
        .map_err(AppError::Database)?;

    Ok(Json(AuditResponse {
        items,
        limit,
        offset,
    }))
}
