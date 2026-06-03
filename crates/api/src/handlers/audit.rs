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
    pub after: Option<String>,
    pub cursor: Option<String>,
    pub actor: Option<String>,
    pub action: Option<String>,
    pub since: Option<DateTime<Utc>>,
}

#[derive(Debug, Serialize)]
pub struct AuditResponse {
    pub items: Vec<AuditEntry>,
    pub limit: i64,
    pub offset: i64,
    pub next_cursor: Option<String>,
}

#[derive(Debug, Clone, Copy)]
struct AuditCursor {
    occurred_at: DateTime<Utc>,
    id: Uuid,
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
    let cursor = query
        .cursor
        .as_deref()
        .or(query.after.as_deref())
        .map(parse_audit_cursor)
        .transpose()?;
    let offset = if cursor.is_some() {
        0
    } else {
        query.offset.unwrap_or(0).max(0)
    };

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
    if let Some(cursor) = cursor {
        builder.push(" AND (occurred_at, id) < (");
        builder.push_bind(cursor.occurred_at);
        builder.push(", ");
        builder.push_bind(cursor.id);
        builder.push(")");
    }

    builder.push(" ORDER BY occurred_at DESC, id DESC LIMIT ");
    builder.push_bind(limit + 1);
    if offset > 0 {
        builder.push(" OFFSET ");
        builder.push_bind(offset);
    }

    let mut items = builder
        .build_query_as::<AuditEntry>()
        .fetch_all(&state.db)
        .await
        .map_err(AppError::Database)?;
    let next_cursor = if items.len() > limit as usize {
        items.pop();
        items.last().map(format_audit_cursor)
    } else {
        None
    };

    Ok(Json(AuditResponse {
        items,
        limit,
        offset,
        next_cursor,
    }))
}

fn parse_audit_cursor(value: &str) -> AppResult<AuditCursor> {
    let Some((occurred_at, id)) = value.split_once('|') else {
        return Err(AppError::Validation("invalid audit cursor".to_owned()));
    };
    let occurred_at = DateTime::parse_from_rfc3339(occurred_at)
        .map_err(|_| AppError::Validation("invalid audit cursor timestamp".to_owned()))?
        .with_timezone(&Utc);
    let id = Uuid::parse_str(id)
        .map_err(|_| AppError::Validation("invalid audit cursor id".to_owned()))?;

    Ok(AuditCursor { occurred_at, id })
}

fn format_audit_cursor(entry: &AuditEntry) -> String {
    format!("{}|{}", entry.occurred_at.to_rfc3339(), entry.id)
}
