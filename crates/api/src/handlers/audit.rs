//! Audit query, export, verification, and discovery endpoints.
//!
//! All list/get/export results have their JSON payload columns
//! (`diff`/`before`/`after`/`metadata`) re-redacted on the way out, so even
//! legacy rows written before audit hardening can never leak a secret through
//! the API.

use axum::{
    extract::{Path, Query, State},
    http::{header, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    Extension, Json,
};
use chrono::{DateTime, Utc};
use common::{
    audit::{
        redact_json, verify_audit_chain, write_audit, AuditChainVerification, AuditEvent,
        RequestContext,
    },
    auth::AuthContext,
    error::AppResult,
    models::{AuditAction, AuditActionInfo, AuditEntry, AUDIT_ENTRY_COLUMNS, AUDIT_SEVERITIES},
    AppError, AppState,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::{Postgres, QueryBuilder};
use uuid::Uuid;

use super::require_workspace;

const DEFAULT_AUDIT_LIMIT: i64 = 20;
const MAX_AUDIT_LIMIT: i64 = 100;
/// Hard ceiling on rows returned by a single export request. Beyond this the
/// caller must narrow the date range; truncation is signalled, never silent.
const MAX_EXPORT_ROWS: i64 = 50_000;

#[derive(Debug, Default, Deserialize)]
pub struct AuditQuery {
    pub limit: Option<i64>,
    pub offset: Option<i64>,
    pub after: Option<String>,
    pub cursor: Option<String>,
    pub actor: Option<String>,
    /// Single action (back-compatible).
    pub action: Option<String>,
    /// Comma-separated list of actions.
    pub actions: Option<String>,
    pub target_type: Option<String>,
    pub target_id: Option<Uuid>,
    pub target_name: Option<String>,
    pub request_id: Option<String>,
    pub correlation_id: Option<String>,
    pub source_ip: Option<String>,
    /// Comma-separated severities.
    pub severity: Option<String>,
    /// Comma-separated categories.
    pub category: Option<String>,
    pub success: Option<bool>,
    /// Back-compatible lower time bound (inclusive).
    pub since: Option<DateTime<Utc>>,
    pub from: Option<DateTime<Utc>>,
    pub to: Option<DateTime<Utc>>,
    /// Free-text search across actor / target / request id / reason.
    pub q: Option<String>,
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

    let mut builder = QueryBuilder::<Postgres>::new(format!(
        "SELECT {AUDIT_ENTRY_COLUMNS} FROM audit_log WHERE workspace_id = "
    ));
    builder.push_bind(id);
    apply_filters(&mut builder, &query);
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

    redact_entries(&mut items);

    Ok(Json(AuditResponse {
        items,
        limit,
        offset,
        next_cursor,
    }))
}

#[axum::debug_handler]
pub async fn get_audit_entry(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthContext>,
    Path((id, audit_id)): Path<(Uuid, Uuid)>,
) -> AppResult<Json<AuditEntry>> {
    require_workspace(&auth, id)?;
    let mut entry = sqlx::query_as::<_, AuditEntry>(&format!(
        "SELECT {AUDIT_ENTRY_COLUMNS} FROM audit_log WHERE workspace_id = $1 AND id = $2"
    ))
    .bind(id)
    .bind(audit_id)
    .fetch_optional(&state.db)
    .await
    .map_err(AppError::Database)?
    .ok_or_else(|| AppError::NotFound {
        resource: format!("audit:{audit_id}"),
    })?;

    redact_entry(&mut entry);
    Ok(Json(entry))
}

#[derive(Debug, Serialize)]
pub struct AuditActionsResponse {
    pub actions: Vec<AuditActionInfo>,
    pub severities: Vec<&'static str>,
    pub categories: Vec<String>,
}

#[axum::debug_handler]
pub async fn list_audit_actions(
    State(_state): State<AppState>,
    Extension(auth): Extension<AuthContext>,
    Path(id): Path<Uuid>,
) -> AppResult<Json<AuditActionsResponse>> {
    require_workspace(&auth, id)?;
    let actions: Vec<AuditActionInfo> = AuditAction::all().iter().map(|a| a.info()).collect();
    let mut categories: Vec<String> = actions.iter().map(|a| a.category.to_owned()).collect();
    categories.sort();
    categories.dedup();
    Ok(Json(AuditActionsResponse {
        actions,
        severities: AUDIT_SEVERITIES.to_vec(),
        categories,
    }))
}

#[axum::debug_handler]
pub async fn verify_audit(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthContext>,
    Path(id): Path<Uuid>,
) -> AppResult<Json<AuditChainVerification>> {
    require_workspace(&auth, id)?;
    let verification = verify_audit_chain(&state.db, id)
        .await
        .map_err(AppError::Database)?;
    Ok(Json(verification))
}

#[derive(Debug, Deserialize)]
pub struct AuditExportQuery {
    #[serde(flatten)]
    pub filters: AuditQuery,
    /// `jsonl` (default) or `csv`.
    pub format: Option<String>,
}

#[axum::debug_handler]
pub async fn export_audit(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthContext>,
    ctx: Option<Extension<RequestContext>>,
    Path(id): Path<Uuid>,
    Query(query): Query<AuditExportQuery>,
) -> AppResult<Response> {
    require_workspace(&auth, id)?;
    let format = query
        .format
        .as_deref()
        .unwrap_or("jsonl")
        .to_ascii_lowercase();
    if format != "jsonl" && format != "csv" {
        return Err(AppError::Validation(
            "format must be 'jsonl' or 'csv'".to_owned(),
        ));
    }

    let mut builder = QueryBuilder::<Postgres>::new(format!(
        "SELECT {AUDIT_ENTRY_COLUMNS} FROM audit_log WHERE workspace_id = "
    ));
    builder.push_bind(id);
    apply_filters(&mut builder, &query.filters);
    builder.push(" ORDER BY occurred_at DESC, id DESC LIMIT ");
    builder.push_bind(MAX_EXPORT_ROWS + 1);

    let mut items = builder
        .build_query_as::<AuditEntry>()
        .fetch_all(&state.db)
        .await
        .map_err(AppError::Database)?;
    let truncated = items.len() as i64 > MAX_EXPORT_ROWS;
    if truncated {
        items.truncate(MAX_EXPORT_ROWS as usize);
        tracing::warn!(
            workspace_id = %id,
            "audit export hit MAX_EXPORT_ROWS; narrow the date range to export everything"
        );
    }
    redact_entries(&mut items);

    // Exporting audit data is itself a required, security-sensitive event.
    let export_event = AuditEvent::new(id, AuditAction::AuditExported, id, "audit_log")
        .actor_api_key(&auth)
        .reason("audit log exported")
        .metadata(serde_json::json!({
            "format": format,
            "rows": items.len(),
            "truncated": truncated,
            "from": query.filters.from.or(query.filters.since),
            "to": query.filters.to,
        }))
        .maybe_request_context(ctx.as_deref());
    write_audit(&state.db, &export_event)
        .await
        .map_err(AppError::Database)?;

    let (body, content_type, filename) = if format == "csv" {
        (
            render_csv(&items),
            "text/csv; charset=utf-8",
            "audit-export.csv",
        )
    } else {
        (
            render_jsonl(&items),
            "application/x-ndjson; charset=utf-8",
            "audit-export.jsonl",
        )
    };

    let mut response = (StatusCode::OK, body).into_response();
    let headers = response.headers_mut();
    if let Ok(value) = HeaderValue::from_str(content_type) {
        headers.insert(header::CONTENT_TYPE, value);
    }
    if let Ok(value) = HeaderValue::from_str(&format!("attachment; filename=\"{filename}\"")) {
        headers.insert(header::CONTENT_DISPOSITION, value);
    }
    if truncated {
        headers.insert("x-audit-export-truncated", HeaderValue::from_static("true"));
    }
    Ok(response)
}

// ── Filters ──────────────────────────────────────────────────────────────────

fn apply_filters(builder: &mut QueryBuilder<'_, Postgres>, query: &AuditQuery) {
    if let Some(actor) = trimmed(&query.actor) {
        builder.push(" AND actor = ");
        builder.push_bind(actor);
    }

    let actions = collect_csv(query.actions.as_deref())
        .into_iter()
        .chain(
            query
                .action
                .iter()
                .filter(|a| !a.trim().is_empty())
                .cloned(),
        )
        .collect::<Vec<_>>();
    if !actions.is_empty() {
        builder.push(" AND action::text = ANY(");
        builder.push_bind(actions);
        builder.push(")");
    }

    if let Some(target_type) = trimmed(&query.target_type) {
        builder.push(" AND target_type = ");
        builder.push_bind(target_type);
    }
    if let Some(target_id) = query.target_id {
        builder.push(" AND target_id = ");
        builder.push_bind(target_id);
    }
    if let Some(target_name) = trimmed(&query.target_name) {
        builder.push(" AND target_name = ");
        builder.push_bind(target_name);
    }
    if let Some(request_id) = trimmed(&query.request_id) {
        builder.push(" AND request_id = ");
        builder.push_bind(request_id);
    }
    if let Some(correlation_id) = trimmed(&query.correlation_id) {
        builder.push(" AND correlation_id = ");
        builder.push_bind(correlation_id);
    }
    if let Some(source_ip) = trimmed(&query.source_ip) {
        builder.push(" AND source_ip = ");
        builder.push_bind(source_ip);
    }

    let severities = collect_csv(query.severity.as_deref());
    if !severities.is_empty() {
        builder.push(" AND severity = ANY(");
        builder.push_bind(severities);
        builder.push(")");
    }
    let categories = collect_csv(query.category.as_deref());
    if !categories.is_empty() {
        builder.push(" AND category = ANY(");
        builder.push_bind(categories);
        builder.push(")");
    }
    if let Some(success) = query.success {
        builder.push(" AND success = ");
        builder.push_bind(success);
    }

    let from = query.from.or(query.since);
    if let Some(from) = from {
        builder.push(" AND occurred_at >= ");
        builder.push_bind(from);
    }
    if let Some(to) = query.to {
        builder.push(" AND occurred_at < ");
        builder.push_bind(to);
    }

    if let Some(q) = trimmed(&query.q) {
        let pattern = format!("%{}%", escape_like(&q));
        builder.push(" AND (actor ILIKE ");
        builder.push_bind(pattern.clone());
        builder.push(" ESCAPE '\\' OR target_name ILIKE ");
        builder.push_bind(pattern.clone());
        builder.push(" ESCAPE '\\' OR target_type ILIKE ");
        builder.push_bind(pattern.clone());
        builder.push(" ESCAPE '\\' OR target_id::text ILIKE ");
        builder.push_bind(pattern.clone());
        builder.push(" ESCAPE '\\' OR COALESCE(request_id, '') ILIKE ");
        builder.push_bind(pattern.clone());
        builder.push(" ESCAPE '\\' OR COALESCE(reason, '') ILIKE ");
        builder.push_bind(pattern);
        builder.push(" ESCAPE '\\')");
    }
}

fn trimmed(value: &Option<String>) -> Option<String> {
    value
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(ToOwned::to_owned)
}

fn collect_csv(value: Option<&str>) -> Vec<String> {
    value
        .map(|raw| {
            raw.split(',')
                .map(str::trim)
                .filter(|v| !v.is_empty())
                .map(ToOwned::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

fn escape_like(input: &str) -> String {
    input
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_")
}

// ── Output redaction (defensive, also covers legacy rows) ─────────────────────

fn redact_entries(items: &mut [AuditEntry]) {
    for item in items.iter_mut() {
        redact_entry(item);
    }
}

fn redact_entry(entry: &mut AuditEntry) {
    for value in [
        &mut entry.diff,
        &mut entry.before,
        &mut entry.after,
        &mut entry.metadata,
    ]
    .into_iter()
    .flatten()
    {
        *value = redact_json(value);
    }
}

// ── Cursor ────────────────────────────────────────────────────────────────────

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

// ── Export rendering ──────────────────────────────────────────────────────────

fn render_jsonl(items: &[AuditEntry]) -> String {
    let mut out = String::new();
    for item in items {
        if let Ok(line) = serde_json::to_string(item) {
            out.push_str(&line);
            out.push('\n');
        }
    }
    out
}

const CSV_COLUMNS: &[&str] = &[
    "occurred_at",
    "id",
    "actor",
    "actor_type",
    "action",
    "category",
    "severity",
    "success",
    "target_type",
    "target_id",
    "target_name",
    "source_ip",
    "request_id",
    "correlation_id",
    "reason",
    "before",
    "after",
    "metadata",
    "diff",
];

fn render_csv(items: &[AuditEntry]) -> String {
    let mut out = String::new();
    out.push_str(&CSV_COLUMNS.join(","));
    out.push('\n');
    for item in items {
        let fields = [
            item.occurred_at.to_rfc3339(),
            item.id.to_string(),
            item.actor.clone(),
            item.actor_type.clone().unwrap_or_default(),
            item.action.as_str().to_owned(),
            item.category.clone().unwrap_or_default(),
            item.severity.clone(),
            item.success.to_string(),
            item.target_type.clone(),
            item.target_id.to_string(),
            item.target_name.clone().unwrap_or_default(),
            item.source_ip.clone().unwrap_or_default(),
            item.request_id.clone().unwrap_or_default(),
            item.correlation_id.clone().unwrap_or_default(),
            item.reason.clone().unwrap_or_default(),
            json_cell(&item.before),
            json_cell(&item.after),
            json_cell(&item.metadata),
            json_cell(&item.diff),
        ];
        let row: Vec<String> = fields.iter().map(|f| csv_escape(f)).collect();
        out.push_str(&row.join(","));
        out.push('\n');
    }
    out
}

fn json_cell(value: &Option<Value>) -> String {
    value.as_ref().map(|v| v.to_string()).unwrap_or_default()
}

fn csv_escape(field: &str) -> String {
    if field.contains(',') || field.contains('"') || field.contains('\n') || field.contains('\r') {
        format!("\"{}\"", field.replace('"', "\"\""))
    } else {
        field.to_owned()
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]
    use super::*;

    #[test]
    fn cursor_round_trips() {
        let occurred_at = DateTime::parse_from_rfc3339("2026-01-02T03:04:05.123456Z")
            .expect("parse")
            .with_timezone(&Utc);
        let id = Uuid::now_v7();
        let entry_cursor = format!("{}|{}", occurred_at.to_rfc3339(), id);
        let parsed = parse_audit_cursor(&entry_cursor).expect("parse cursor");
        assert_eq!(parsed.id, id);
        assert_eq!(parsed.occurred_at, occurred_at);
    }

    #[test]
    fn invalid_cursor_is_rejected() {
        assert!(parse_audit_cursor("not-a-cursor").is_err());
        assert!(parse_audit_cursor("2026-01-02T03:04:05Z|not-a-uuid").is_err());
    }

    #[test]
    fn collect_csv_splits_and_trims() {
        assert_eq!(
            collect_csv(Some("key_revoked, tool_secret_revealed ,")),
            vec!["key_revoked".to_owned(), "tool_secret_revealed".to_owned()]
        );
        assert!(collect_csv(None).is_empty());
        assert!(collect_csv(Some("  ,  ")).is_empty());
    }

    #[test]
    fn escape_like_escapes_wildcards() {
        assert_eq!(escape_like("a%b_c\\d"), "a\\%b\\_c\\\\d");
    }

    #[test]
    fn csv_escape_quotes_special_chars() {
        assert_eq!(csv_escape("plain"), "plain");
        assert_eq!(csv_escape("a,b"), "\"a,b\"");
        assert_eq!(csv_escape("a\"b"), "\"a\"\"b\"");
    }
}
