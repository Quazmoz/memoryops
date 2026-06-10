use std::collections::HashMap;

use axum::{extract::Path, extract::Query, extract::State, Extension, Json};
use chrono::{DateTime, Utc};
use common::{
    audit::spawn_audit_log,
    auth::AuthContext,
    error::AppResult,
    models::AuditAction,
    services::{invoke_workspace_skill, SkillInvocationSource, WorkspaceConfigService},
    AppError, AppState,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::{PgPool, Postgres, QueryBuilder};
use uuid::Uuid;

use crate::security::{decrypt_secret_legacy_or_current, encrypt_secret};

use super::require_workspace;

/// Column list for SELECT/RETURNING on workspace_tools.
const TOOL_COLUMNS: &str = "id, workspace_id, name, description, endpoint_url, http_method, \
     input_schema, output_schema, auth_header, enabled, version, \
     scope_visibility, rate_limit_per_minute, circuit_breaker_threshold, \
     circuit_breaker_cooldown_seconds, created_at, updated_at";

/// Column list for SELECT on workspace_tool_versions.
const TOOL_VERSION_COLUMNS: &str =
    "id, tool_id, workspace_id, name, version, description, endpoint_url, \
     http_method, input_schema, output_schema, auth_header, enabled, \
     scope_visibility, change_note, created_by, created_at";

const DEFAULT_TOOL_LIMIT: i64 = 50;
const MAX_TOOL_LIMIT: i64 = 100;

#[derive(Debug, Deserialize)]
pub struct ToolListQuery {
    pub after: Option<Uuid>,
    pub limit: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct CreateToolRequest {
    pub name: String,
    pub description: String,
    pub endpoint_url: String,
    pub http_method: Option<HttpMethod>,
    pub input_schema: Option<Value>,
    pub output_schema: Option<Value>,
    pub auth_header: Option<String>,
    pub auth_secret: Option<String>,
    pub change_note: Option<String>,
    pub scope_visibility: Option<ToolScopeVisibility>,
    pub rate_limit_per_minute: Option<i32>,
    pub circuit_breaker_threshold: Option<i32>,
    pub circuit_breaker_cooldown_seconds: Option<i32>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateToolRequest {
    pub description: Option<String>,
    pub endpoint_url: Option<String>,
    pub http_method: Option<HttpMethod>,
    pub input_schema: Option<Value>,
    pub output_schema: Option<Value>,
    pub auth_header: Option<String>,
    pub auth_secret: Option<String>,
    pub enabled: Option<bool>,
    pub change_note: Option<String>,
    pub scope_visibility: Option<ToolScopeVisibility>,
    pub rate_limit_per_minute: Option<i32>,
    pub circuit_breaker_threshold: Option<i32>,
    pub circuit_breaker_cooldown_seconds: Option<i32>,
}

#[derive(Debug, Deserialize)]
pub struct RollbackToolRequest {
    pub change_note: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct InvokeToolRequest {
    pub body: Option<Value>,
    pub headers: Option<HashMap<String, String>>,
    pub version: Option<i32>,
}

#[derive(Debug, Deserialize)]
pub struct ImportToolsRequest {
    pub tools: Vec<ImportToolItem>,
    #[serde(default)]
    pub overwrite: bool,
}

#[derive(Debug, Deserialize)]
pub struct ImportToolItem {
    pub name: String,
    pub description: String,
    pub endpoint_url: String,
    pub http_method: Option<HttpMethod>,
    pub input_schema: Option<Value>,
    pub output_schema: Option<Value>,
    pub auth_header: Option<String>,
    pub auth_secret: Option<String>,
    pub enabled: Option<bool>,
    pub scope_visibility: Option<ToolScopeVisibility>,
    pub rate_limit_per_minute: Option<i32>,
    pub circuit_breaker_threshold: Option<i32>,
    pub circuit_breaker_cooldown_seconds: Option<i32>,
}

#[derive(Debug, Serialize)]
pub struct ImportToolsResponse {
    pub created: usize,
    pub updated: usize,
    pub skipped: usize,
    pub errors: Vec<ImportToolError>,
}

#[derive(Debug, Serialize)]
pub struct ImportToolError {
    pub name: String,
    pub error: String,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct ExportedTool {
    pub name: String,
    pub description: String,
    pub endpoint_url: String,
    pub http_method: String,
    pub input_schema: Value,
    pub output_schema: Value,
    pub auth_header: Option<String>,
    pub enabled: bool,
    pub scope_visibility: String,
    pub rate_limit_per_minute: i32,
    pub circuit_breaker_threshold: i32,
    pub circuit_breaker_cooldown_seconds: i32,
    pub version: i32,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct ToolInvocation {
    pub id: i64,
    pub tool_id: Uuid,
    pub workspace_id: Uuid,
    pub tool_name: String,
    pub tool_version: i32,
    pub actor: String,
    pub source: String,
    pub status_code: i32,
    pub latency_ms: i32,
    pub error: Option<String>,
    pub occurred_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct InvocationListQuery {
    pub limit: Option<i64>,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, sqlx::Type)]
#[serde(rename_all = "lowercase")]
#[sqlx(type_name = "TEXT", rename_all = "lowercase")]
pub enum ToolScopeVisibility {
    Private,
    Workspace,
    Published,
}

impl ToolScopeVisibility {
    fn as_str(self) -> &'static str {
        match self {
            ToolScopeVisibility::Private => "private",
            ToolScopeVisibility::Workspace => "workspace",
            ToolScopeVisibility::Published => "published",
        }
    }
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum HttpMethod {
    Get,
    Post,
    Put,
}

impl HttpMethod {
    fn as_str(self) -> &'static str {
        match self {
            HttpMethod::Get => "GET",
            HttpMethod::Post => "POST",
            HttpMethod::Put => "PUT",
        }
    }
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct ToolResponse {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub name: String,
    pub description: String,
    pub endpoint_url: String,
    pub http_method: String,
    pub input_schema: Value,
    pub output_schema: Value,
    pub auth_header: Option<String>,
    pub enabled: bool,
    pub version: i32,
    pub scope_visibility: String,
    pub rate_limit_per_minute: i32,
    pub circuit_breaker_threshold: i32,
    pub circuit_breaker_cooldown_seconds: i32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct ToolVersionResponse {
    pub id: Uuid,
    pub tool_id: Uuid,
    pub workspace_id: Uuid,
    pub name: String,
    pub version: i32,
    pub description: String,
    pub endpoint_url: String,
    pub http_method: String,
    pub input_schema: Value,
    pub output_schema: Value,
    pub auth_header: Option<String>,
    pub enabled: bool,
    pub scope_visibility: String,
    pub change_note: Option<String>,
    pub created_by: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
pub struct ToolSecretResponse {
    pub auth_header: Option<String>,
    pub plaintext_secret: String,
}

#[axum::debug_handler]
pub async fn create_tool(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthContext>,
    Path(id): Path<Uuid>,
    Json(request): Json<CreateToolRequest>,
) -> AppResult<Json<ToolResponse>> {
    require_workspace(&auth, id)?;
    validate_name(&request.name)?;
    validate_description(&request.description)?;
    validate_endpoint_url(&request.endpoint_url)?;
    validate_endpoint_url_dns(&request.endpoint_url).await?;
    validate_schema(request.input_schema.as_ref(), "input_schema")?;
    validate_schema(request.output_schema.as_ref(), "output_schema")?;
    let auth_header = normalized_optional_text(request.auth_header.as_deref());
    let auth_secret_enc = encrypted_secret(&state, request.auth_secret.as_deref())?;
    validate_auth_pair(auth_header.as_ref(), auth_secret_enc.as_ref())?;
    let change_note = normalized_optional_text(request.change_note.as_deref());

    enforce_change_note_for_compliance(&state, id, change_note.as_deref()).await?;

    let mut tx = state.db.begin().await.map_err(AppError::Database)?;

    let tool = sqlx::query_as::<_, ToolResponse>(&format!(
        r#"
        INSERT INTO workspace_tools (
            workspace_id, name, description, endpoint_url, http_method,
            input_schema, output_schema, auth_header, auth_secret_enc,
            scope_visibility, rate_limit_per_minute,
            circuit_breaker_threshold, circuit_breaker_cooldown_seconds
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9,
                COALESCE($10, 'workspace'),
                COALESCE($11, 0),
                COALESCE($12, 0),
                COALESCE($13, 60))
        RETURNING {TOOL_COLUMNS}
        "#
    ))
    .bind(id)
    .bind(request.name.trim())
    .bind(request.description.trim())
    .bind(request.endpoint_url.trim())
    .bind(request.http_method.unwrap_or(HttpMethod::Post).as_str())
    .bind(request.input_schema.unwrap_or_else(|| json!({})))
    .bind(request.output_schema.unwrap_or_else(|| json!({})))
    .bind(auth_header)
    .bind(auth_secret_enc)
    .bind(request.scope_visibility.map(ToolScopeVisibility::as_str))
    .bind(request.rate_limit_per_minute)
    .bind(request.circuit_breaker_threshold)
    .bind(request.circuit_breaker_cooldown_seconds)
    .fetch_one(&mut *tx)
    .await
    .map_err(map_tool_insert_error)?;

    insert_tool_version_snapshot(
        &mut tx,
        &tool,
        change_note.as_deref(),
        Some(auth.actor().as_str()),
    )
    .await?;

    tx.commit().await.map_err(AppError::Database)?;

    spawn_audit_log(
        state.db.clone(),
        id,
        auth.actor(),
        AuditAction::ToolCreated,
        tool.id,
        "workspace_tool",
        Some(json!({ "name": tool.name, "version": tool.version })),
    );

    Ok(Json(tool))
}

#[axum::debug_handler]
pub async fn list_tools(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthContext>,
    Path(id): Path<Uuid>,
    Query(query): Query<ToolListQuery>,
) -> AppResult<Json<Vec<ToolResponse>>> {
    require_workspace(&auth, id)?;
    let limit = query
        .limit
        .unwrap_or(DEFAULT_TOOL_LIMIT)
        .clamp(1, MAX_TOOL_LIMIT);
    let mut builder = QueryBuilder::<Postgres>::new(format!(
        "SELECT {TOOL_COLUMNS} FROM workspace_tools WHERE workspace_id = "
    ));
    builder.push_bind(id);

    if let Some(after) = query.after {
        builder.push(
            " AND (created_at, id) < (SELECT created_at, id FROM workspace_tools WHERE workspace_id = ",
        );
        builder.push_bind(id);
        builder.push(" AND id = ");
        builder.push_bind(after);
        builder.push(")");
    }

    builder.push(" ORDER BY created_at DESC, id DESC LIMIT ");
    builder.push_bind(limit);

    let tools = builder
        .build_query_as::<ToolResponse>()
        .fetch_all(&state.db)
        .await
        .map_err(AppError::Database)?;

    Ok(Json(tools))
}

#[axum::debug_handler]
pub async fn get_tool(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthContext>,
    Path((id, name)): Path<(Uuid, String)>,
) -> AppResult<Json<ToolResponse>> {
    require_workspace(&auth, id)?;
    let tool = fetch_tool(&state, id, &name).await?;
    Ok(Json(tool))
}

#[axum::debug_handler]
pub async fn update_tool(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthContext>,
    Path((id, name)): Path<(Uuid, String)>,
    Json(request): Json<UpdateToolRequest>,
) -> AppResult<Json<ToolResponse>> {
    require_workspace(&auth, id)?;
    if let Some(description) = &request.description {
        validate_description(description)?;
    }
    if let Some(endpoint_url) = &request.endpoint_url {
        validate_endpoint_url(endpoint_url)?;
        validate_endpoint_url_dns(endpoint_url).await?;
    }
    validate_schema(request.input_schema.as_ref(), "input_schema")?;
    validate_schema(request.output_schema.as_ref(), "output_schema")?;

    let auth_header = request
        .auth_header
        .as_deref()
        .and_then(|value| normalized_optional_text(Some(value)));
    let auth_secret_enc = encrypted_secret(&state, request.auth_secret.as_deref())?;
    let change_note = normalized_optional_text(request.change_note.as_deref());

    enforce_change_note_for_compliance(&state, id, change_note.as_deref()).await?;

    let mut tx = state.db.begin().await.map_err(AppError::Database)?;

    if auth_secret_enc.is_some() && auth_header.is_none() {
        let existing_header = sqlx::query_scalar::<_, Option<String>>(
            "SELECT auth_header FROM workspace_tools WHERE workspace_id = $1 AND name = $2",
        )
        .bind(id)
        .bind(&name)
        .fetch_optional(&mut *tx)
        .await
        .map_err(AppError::Database)?
        .flatten();
        validate_auth_pair(existing_header.as_ref(), auth_secret_enc.as_ref())?;
    }

    let tool = sqlx::query_as::<_, ToolResponse>(&format!(
        r#"
        UPDATE workspace_tools
        SET description = COALESCE($3, description),
            endpoint_url = COALESCE($4, endpoint_url),
            http_method = COALESCE($5, http_method),
            input_schema = COALESCE($6, input_schema),
            output_schema = COALESCE($7, output_schema),
            auth_header = COALESCE($8, auth_header),
            auth_secret_enc = COALESCE($9, auth_secret_enc),
            enabled = COALESCE($10, enabled),
            scope_visibility = COALESCE($11, scope_visibility),
            rate_limit_per_minute = COALESCE($12, rate_limit_per_minute),
            circuit_breaker_threshold = COALESCE($13, circuit_breaker_threshold),
            circuit_breaker_cooldown_seconds = COALESCE($14, circuit_breaker_cooldown_seconds),
            version = version + 1
        WHERE workspace_id = $1 AND name = $2
        RETURNING {TOOL_COLUMNS}
        "#
    ))
    .bind(id)
    .bind(&name)
    .bind(request.description.as_deref().map(str::trim))
    .bind(request.endpoint_url.as_deref().map(str::trim))
    .bind(request.http_method.map(HttpMethod::as_str))
    .bind(request.input_schema)
    .bind(request.output_schema)
    .bind(auth_header)
    .bind(auth_secret_enc)
    .bind(request.enabled)
    .bind(request.scope_visibility.map(ToolScopeVisibility::as_str))
    .bind(request.rate_limit_per_minute)
    .bind(request.circuit_breaker_threshold)
    .bind(request.circuit_breaker_cooldown_seconds)
    .fetch_optional(&mut *tx)
    .await
    .map_err(AppError::Database)?
    .ok_or_else(|| AppError::NotFound {
        resource: format!("workspace_tool:{name}"),
    })?;

    insert_tool_version_snapshot(
        &mut tx,
        &tool,
        change_note.as_deref(),
        Some(auth.actor().as_str()),
    )
    .await?;

    tx.commit().await.map_err(AppError::Database)?;

    spawn_audit_log(
        state.db.clone(),
        id,
        auth.actor(),
        AuditAction::ToolUpdated,
        tool.id,
        "workspace_tool",
        Some(json!({
            "name": tool.name,
            "version": tool.version,
            "change_note": change_note,
        })),
    );

    Ok(Json(tool))
}

#[axum::debug_handler]
pub async fn delete_tool(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthContext>,
    Path((id, name)): Path<(Uuid, String)>,
) -> AppResult<Json<Value>> {
    require_workspace(&auth, id)?;
    let row: Option<(Uuid,)> = sqlx::query_as(
        "DELETE FROM workspace_tools WHERE workspace_id = $1 AND name = $2 RETURNING id",
    )
    .bind(id)
    .bind(&name)
    .fetch_optional(&state.db)
    .await
    .map_err(AppError::Database)?;
    let Some((tool_id,)) = row else {
        return Err(AppError::NotFound {
            resource: format!("workspace_tool:{name}"),
        });
    };

    spawn_audit_log(
        state.db.clone(),
        id,
        auth.actor(),
        AuditAction::ToolDeleted,
        tool_id,
        "workspace_tool",
        Some(json!({ "name": name })),
    );

    Ok(Json(json!({ "deleted": true })))
}

#[axum::debug_handler]
pub async fn get_tool_secret(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthContext>,
    Path((id, name)): Path<(Uuid, String)>,
) -> AppResult<Json<ToolSecretResponse>> {
    require_workspace(&auth, id)?;

    #[derive(Debug, sqlx::FromRow)]
    struct SecretRow {
        auth_header: Option<String>,
        auth_secret_enc: Option<String>,
    }

    let row = sqlx::query_as::<_, SecretRow>(
        "SELECT auth_header, auth_secret_enc FROM workspace_tools WHERE workspace_id = $1 AND name = $2",
    )
    .bind(id)
    .bind(&name)
    .fetch_optional(&state.db)
    .await
    .map_err(AppError::Database)?
    .ok_or_else(|| AppError::NotFound {
        resource: format!("workspace_tool:{name}"),
    })?;
    let ciphertext = row.auth_secret_enc.ok_or_else(|| AppError::NotFound {
        resource: format!("workspace_tool_secret:{name}"),
    })?;
    let decrypted =
        decrypt_secret_legacy_or_current(state.app_secret_key.as_ref().as_str(), &ciphertext)?;
    persist_migrated_ciphertext(&state.db, id, &name, &decrypted).await?;

    Ok(Json(ToolSecretResponse {
        auth_header: row.auth_header,
        plaintext_secret: decrypted.plaintext,
    }))
}

async fn fetch_tool(state: &AppState, workspace_id: Uuid, name: &str) -> AppResult<ToolResponse> {
    sqlx::query_as::<_, ToolResponse>(&format!(
        "SELECT {TOOL_COLUMNS} FROM workspace_tools WHERE workspace_id = $1 AND name = $2"
    ))
    .bind(workspace_id)
    .bind(name)
    .fetch_optional(&state.db)
    .await
    .map_err(AppError::Database)?
    .ok_or_else(|| AppError::NotFound {
        resource: format!("workspace_tool:{name}"),
    })
}

async fn insert_tool_version_snapshot(
    tx: &mut sqlx::Transaction<'_, Postgres>,
    tool: &ToolResponse,
    change_note: Option<&str>,
    created_by: Option<&str>,
) -> AppResult<()> {
    let auth_secret_enc = sqlx::query_scalar::<_, Option<String>>(
        "SELECT auth_secret_enc FROM workspace_tools WHERE id = $1",
    )
    .bind(tool.id)
    .fetch_one(&mut **tx)
    .await
    .map_err(AppError::Database)?;

    sqlx::query(
        r#"
        INSERT INTO workspace_tool_versions (
            tool_id, workspace_id, name, version, description, endpoint_url,
            http_method, input_schema, output_schema, auth_header, auth_secret_enc,
            enabled, change_note, created_by, scope_visibility
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15)
        "#,
    )
    .bind(tool.id)
    .bind(tool.workspace_id)
    .bind(&tool.name)
    .bind(tool.version)
    .bind(&tool.description)
    .bind(&tool.endpoint_url)
    .bind(&tool.http_method)
    .bind(&tool.input_schema)
    .bind(&tool.output_schema)
    .bind(tool.auth_header.as_deref())
    .bind(auth_secret_enc)
    .bind(tool.enabled)
    .bind(change_note)
    .bind(created_by)
    .bind(&tool.scope_visibility)
    .execute(&mut **tx)
    .await
    .map_err(AppError::Database)?;

    Ok(())
}

#[axum::debug_handler]
pub async fn list_tool_versions(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthContext>,
    Path((id, name)): Path<(Uuid, String)>,
) -> AppResult<Json<Vec<ToolVersionResponse>>> {
    require_workspace(&auth, id)?;

    let versions = sqlx::query_as::<_, ToolVersionResponse>(&format!(
        r#"
        SELECT {TOOL_VERSION_COLUMNS}
        FROM workspace_tool_versions
        WHERE workspace_id = $1 AND name = $2
        ORDER BY version DESC
        "#
    ))
    .bind(id)
    .bind(&name)
    .fetch_all(&state.db)
    .await
    .map_err(AppError::Database)?;

    if versions.is_empty() {
        // Distinguish unknown tool from empty history (should not happen post-create).
        let _ = fetch_tool(&state, id, &name).await?;
    }

    Ok(Json(versions))
}

#[axum::debug_handler]
pub async fn get_tool_version(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthContext>,
    Path((id, name, version)): Path<(Uuid, String, i32)>,
) -> AppResult<Json<ToolVersionResponse>> {
    require_workspace(&auth, id)?;

    let row = sqlx::query_as::<_, ToolVersionResponse>(&format!(
        r#"
        SELECT {TOOL_VERSION_COLUMNS}
        FROM workspace_tool_versions
        WHERE workspace_id = $1 AND name = $2 AND version = $3
        "#
    ))
    .bind(id)
    .bind(&name)
    .bind(version)
    .fetch_optional(&state.db)
    .await
    .map_err(AppError::Database)?
    .ok_or_else(|| AppError::NotFound {
        resource: format!("workspace_tool_version:{name}@{version}"),
    })?;

    Ok(Json(row))
}

#[axum::debug_handler]
pub async fn rollback_tool_version(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthContext>,
    Path((id, name, version)): Path<(Uuid, String, i32)>,
    Json(request): Json<RollbackToolRequest>,
) -> AppResult<Json<ToolResponse>> {
    require_workspace(&auth, id)?;

    let change_note = normalized_optional_text(request.change_note.as_deref());
    enforce_change_note_for_compliance(&state, id, change_note.as_deref()).await?;

    let mut tx = state.db.begin().await.map_err(AppError::Database)?;

    #[derive(sqlx::FromRow)]
    struct VersionSnapshot {
        description: String,
        endpoint_url: String,
        http_method: String,
        input_schema: Value,
        output_schema: Value,
        auth_header: Option<String>,
        auth_secret_enc: Option<String>,
        enabled: bool,
        scope_visibility: String,
    }

    let snapshot = sqlx::query_as::<_, VersionSnapshot>(
        r#"
        SELECT description, endpoint_url, http_method, input_schema, output_schema,
               auth_header, auth_secret_enc, enabled, scope_visibility
        FROM workspace_tool_versions
        WHERE workspace_id = $1 AND name = $2 AND version = $3
        "#,
    )
    .bind(id)
    .bind(&name)
    .bind(version)
    .fetch_optional(&mut *tx)
    .await
    .map_err(AppError::Database)?
    .ok_or_else(|| AppError::NotFound {
        resource: format!("workspace_tool_version:{name}@{version}"),
    })?;

    // Validate the snapshot's URL at rollback time (defends against newly-blocked hosts).
    validate_endpoint_url(&snapshot.endpoint_url)?;
    validate_endpoint_url_dns(&snapshot.endpoint_url).await?;

    let tool = sqlx::query_as::<_, ToolResponse>(&format!(
        r#"
        UPDATE workspace_tools
        SET description = $3,
            endpoint_url = $4,
            http_method = $5,
            input_schema = $6,
            output_schema = $7,
            auth_header = $8,
            auth_secret_enc = $9,
            enabled = $10,
            scope_visibility = $11,
            version = version + 1
        WHERE workspace_id = $1 AND name = $2
        RETURNING {TOOL_COLUMNS}
        "#
    ))
    .bind(id)
    .bind(&name)
    .bind(&snapshot.description)
    .bind(&snapshot.endpoint_url)
    .bind(&snapshot.http_method)
    .bind(&snapshot.input_schema)
    .bind(&snapshot.output_schema)
    .bind(snapshot.auth_header.as_deref())
    .bind(snapshot.auth_secret_enc.as_deref())
    .bind(snapshot.enabled)
    .bind(&snapshot.scope_visibility)
    .fetch_optional(&mut *tx)
    .await
    .map_err(AppError::Database)?
    .ok_or_else(|| AppError::NotFound {
        resource: format!("workspace_tool:{name}"),
    })?;

    let note = change_note.unwrap_or_else(|| format!("rollback to v{version}"));
    insert_tool_version_snapshot(&mut tx, &tool, Some(&note), Some(auth.actor().as_str())).await?;

    tx.commit().await.map_err(AppError::Database)?;

    spawn_audit_log(
        state.db.clone(),
        id,
        auth.actor(),
        AuditAction::ToolRolledBack,
        tool.id,
        "workspace_tool",
        Some(json!({
            "name": tool.name,
            "version": tool.version,
            "rolled_back_to": version,
        })),
    );

    Ok(Json(tool))
}

fn validate_name(value: &str) -> AppResult<()> {
    let trimmed = value.trim();
    if trimmed.is_empty() || trimmed.len() > 64 {
        return Err(AppError::Validation(
            "tool name must be 1-64 characters".to_owned(),
        ));
    }
    let mut chars = trimmed.chars();
    let Some(first) = chars.next() else {
        return Err(AppError::Validation("tool name is required".to_owned()));
    };
    if !first.is_ascii_lowercase() {
        return Err(AppError::Validation(
            "tool name must start with a lowercase letter".to_owned(),
        ));
    }
    if chars.any(|ch| !(ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '_')) {
        return Err(AppError::Validation(
            "tool name must match ^[a-z][a-z0-9_]{0,63}$".to_owned(),
        ));
    }
    Ok(())
}

fn validate_description(value: &str) -> AppResult<()> {
    let length = value.trim().chars().count();
    if !(1..=500).contains(&length) {
        return Err(AppError::Validation(
            "tool description must be 1-500 characters".to_owned(),
        ));
    }
    Ok(())
}

fn validate_endpoint_url(value: &str) -> AppResult<()> {
    let url = reqwest::Url::parse(value.trim())
        .map_err(|_| AppError::Validation("tool endpoint_url is not a valid URL".to_owned()))?;

    if url.scheme() != "https" {
        return Err(AppError::Validation(
            "tool endpoint_url must use the https scheme".to_owned(),
        ));
    }

    if !url.username().is_empty() || url.password().is_some() {
        return Err(AppError::Validation(
            "tool endpoint_url must not contain credentials".to_owned(),
        ));
    }

    let host = url
        .host_str()
        .ok_or_else(|| AppError::Validation("tool endpoint_url must have a host".to_owned()))?;

    reject_forbidden_host(host)?;

    Ok(())
}

/// Reject hostnames / IP literals that would cause SSRF.
/// This is a syntax-level check; DNS-resolved IPs are validated separately in
/// [`validate_endpoint_url_dns`] at request time.
fn reject_forbidden_host(host: &str) -> AppResult<()> {
    let lower = host.to_ascii_lowercase();

    if lower == "localhost" || lower.ends_with(".localhost") {
        return Err(AppError::Validation(
            "tool endpoint_url host is not permitted".to_owned(),
        ));
    }

    let ip_str = if lower.starts_with('[') && lower.ends_with(']') {
        &lower[1..lower.len() - 1]
    } else {
        &lower
    };

    // Parse as an IP literal (IPv4 or bracket-stripped IPv6)
    if let Ok(ip) = ip_str.parse::<std::net::IpAddr>() {
        reject_forbidden_ip(ip)?;
    }

    Ok(())
}

/// Reject loopback, RFC-1918 private, link-local, documentation, unspecified,
/// broadcast, multicast, and IPv4-mapped-in-IPv6 addresses.
fn reject_forbidden_ip(ip: std::net::IpAddr) -> AppResult<()> {
    match ip {
        std::net::IpAddr::V4(v4) => {
            if v4.is_loopback()
                || v4.is_private()
                || v4.is_link_local()
                || v4.is_broadcast()
                || v4.is_documentation()
                || v4.is_unspecified()
                || v4.is_multicast()
            {
                return Err(AppError::Validation(
                    "tool endpoint_url resolves to a forbidden address".to_owned(),
                ));
            }
        }
        std::net::IpAddr::V6(v6) => {
            if v6.is_loopback() || v6.is_multicast() || v6.is_unspecified() {
                return Err(AppError::Validation(
                    "tool endpoint_url resolves to a forbidden address".to_owned(),
                ));
            }
            // Reject IPv4-mapped / IPv4-compatible addresses (e.g. ::ffff:10.0.0.1)
            if let Some(v4) = v6.to_ipv4() {
                reject_forbidden_ip(std::net::IpAddr::V4(v4))?;
            }
        }
    }
    Ok(())
}

/// Resolve the hostname of `url` via DNS and reject any address in a forbidden
/// range.  This prevents SSRF via DNS rebinding (the URL passes the syntax
/// check but resolves to an internal address at call time).
async fn validate_endpoint_url_dns(url: &str) -> AppResult<()> {
    let parsed = reqwest::Url::parse(url)
        .map_err(|_| AppError::Validation("invalid endpoint URL".to_owned()))?;

    let host = parsed.host_str().unwrap_or("");

    // IP literals are already validated by the syntax check; skip DNS.
    if host.parse::<std::net::IpAddr>().is_ok() {
        return Ok(());
    }

    let port = parsed.port_or_known_default().unwrap_or(443);
    let addr_str = format!("{host}:{port}");

    let addrs = tokio::net::lookup_host(&*addr_str)
        .await
        .map_err(|_| AppError::Validation("tool endpoint_url could not be resolved".to_owned()))?;

    for sock_addr in addrs {
        reject_forbidden_ip(sock_addr.ip())?;
    }

    Ok(())
}

fn validate_schema(value: Option<&Value>, field: &'static str) -> AppResult<()> {
    if value.is_some_and(|schema| !schema.is_object()) {
        return Err(AppError::Validation(format!(
            "{field} must be a JSON object"
        )));
    }
    Ok(())
}

fn validate_auth_pair(
    auth_header: Option<&String>,
    auth_secret_enc: Option<&String>,
) -> AppResult<()> {
    if auth_secret_enc.is_some() && auth_header.is_none() {
        return Err(AppError::Validation(
            "auth_header is required when auth_secret is provided".to_owned(),
        ));
    }
    Ok(())
}

fn normalized_optional_text(value: Option<&str>) -> Option<String> {
    value.and_then(|text| {
        let trimmed = text.trim();
        (!trimmed.is_empty()).then(|| trimmed.to_owned())
    })
}

fn encrypted_secret(state: &AppState, value: Option<&str>) -> AppResult<Option<String>> {
    let Some(secret) = normalized_optional_text(value) else {
        return Ok(None);
    };
    encrypt_secret(state.app_secret_key.as_ref().as_str(), &secret).map(Some)
}

async fn persist_migrated_ciphertext(
    db: &PgPool,
    workspace_id: Uuid,
    name: &str,
    decrypted: &crate::security::DecryptedSecret,
) -> AppResult<()> {
    let Some(migrated_ciphertext) = decrypted.migrated_ciphertext.as_ref() else {
        return Ok(());
    };

    sqlx::query(
        "UPDATE workspace_tools SET auth_secret_enc = $3 WHERE workspace_id = $1 AND name = $2",
    )
    .bind(workspace_id)
    .bind(name)
    .bind(migrated_ciphertext)
    .execute(db)
    .await
    .map_err(AppError::Database)?;

    Ok(())
}

/// Request body for the tool test proxy.
#[derive(Debug, Deserialize)]
pub struct ToolTestRequest {
    pub body: Option<Value>,
    pub headers: Option<HashMap<String, String>>,
    pub version: Option<i32>,
}

/// Response from the tool test proxy.
#[derive(Debug, Serialize)]
pub struct ToolTestResponse {
    pub status: u16,
    pub latency_ms: u64,
    pub body: Value,
}

#[axum::debug_handler]
pub async fn test_tool(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthContext>,
    Path((id, name)): Path<(Uuid, String)>,
    Json(request): Json<ToolTestRequest>,
) -> AppResult<Json<ToolTestResponse>> {
    require_workspace(&auth, id)?;
    let (response, _version) = invoke_tool_core(
        &state,
        id,
        &name,
        request.body.as_ref(),
        request.headers.as_ref(),
        SkillInvocationSource::Test,
        &auth.actor(),
        request.version,
    )
    .await?;
    Ok(Json(response))
}

#[axum::debug_handler]
pub async fn invoke_tool(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthContext>,
    Path((id, name)): Path<(Uuid, String)>,
    Json(request): Json<InvokeToolRequest>,
) -> AppResult<Json<ToolTestResponse>> {
    require_workspace(&auth, id)?;
    let (response, _version) = invoke_tool_core(
        &state,
        id,
        &name,
        request.body.as_ref(),
        request.headers.as_ref(),
        SkillInvocationSource::Http,
        &auth.actor(),
        request.version,
    )
    .await?;
    Ok(Json(response))
}

#[axum::debug_handler]
pub async fn list_tool_invocations(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthContext>,
    Path((id, name)): Path<(Uuid, String)>,
    Query(query): Query<InvocationListQuery>,
) -> AppResult<Json<Vec<ToolInvocation>>> {
    require_workspace(&auth, id)?;
    let limit = query.limit.unwrap_or(50).clamp(1, 500);
    let rows = sqlx::query_as::<_, ToolInvocation>(
        r#"
        SELECT id, tool_id, workspace_id, tool_name, tool_version, actor,
               source, status_code, latency_ms, error, occurred_at
        FROM workspace_tool_invocations
        WHERE workspace_id = $1 AND tool_name = $2
        ORDER BY occurred_at DESC
        LIMIT $3
        "#,
    )
    .bind(id)
    .bind(&name)
    .bind(limit)
    .fetch_all(&state.db)
    .await
    .map_err(AppError::Database)?;
    Ok(Json(rows))
}

#[axum::debug_handler]
pub async fn export_tools(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthContext>,
    Path(id): Path<Uuid>,
) -> AppResult<Json<Vec<ExportedTool>>> {
    require_workspace(&auth, id)?;
    let rows = sqlx::query_as::<_, ExportedTool>(
        r#"
        SELECT name, description, endpoint_url, http_method, input_schema,
               output_schema, auth_header, enabled, scope_visibility,
               rate_limit_per_minute, circuit_breaker_threshold,
               circuit_breaker_cooldown_seconds, version
        FROM workspace_tools
        WHERE workspace_id = $1
        ORDER BY name
        "#,
    )
    .bind(id)
    .fetch_all(&state.db)
    .await
    .map_err(AppError::Database)?;
    Ok(Json(rows))
}

#[axum::debug_handler]
pub async fn import_tools(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthContext>,
    Path(id): Path<Uuid>,
    Json(request): Json<ImportToolsRequest>,
) -> AppResult<Json<ImportToolsResponse>> {
    require_workspace(&auth, id)?;

    let mut created = 0usize;
    let mut updated = 0usize;
    let mut skipped = 0usize;
    let mut errors = Vec::new();

    for item in request.tools {
        let name = item.name.trim().to_string();
        match import_one_tool(&state, &auth, id, item, request.overwrite).await {
            Ok(Some(true)) => created += 1,
            Ok(Some(false)) => updated += 1,
            Ok(None) => skipped += 1,
            Err(error) => errors.push(ImportToolError {
                name,
                error: format!("{error}"),
            }),
        }
    }

    Ok(Json(ImportToolsResponse {
        created,
        updated,
        skipped,
        errors,
    }))
}

async fn import_one_tool(
    state: &AppState,
    auth: &AuthContext,
    workspace_id: Uuid,
    item: ImportToolItem,
    overwrite: bool,
) -> AppResult<Option<bool>> {
    validate_name(&item.name)?;
    validate_description(&item.description)?;
    validate_endpoint_url(&item.endpoint_url)?;
    validate_endpoint_url_dns(&item.endpoint_url).await?;
    validate_schema(item.input_schema.as_ref(), "input_schema")?;
    validate_schema(item.output_schema.as_ref(), "output_schema")?;

    let auth_header = normalized_optional_text(item.auth_header.as_deref());
    let auth_secret_enc = encrypted_secret(state, item.auth_secret.as_deref())?;
    validate_auth_pair(auth_header.as_ref(), auth_secret_enc.as_ref())?;

    let exists: Option<(Uuid,)> =
        sqlx::query_as("SELECT id FROM workspace_tools WHERE workspace_id = $1 AND name = $2")
            .bind(workspace_id)
            .bind(&item.name)
            .fetch_optional(&state.db)
            .await
            .map_err(AppError::Database)?;

    if exists.is_some() && !overwrite {
        return Ok(None);
    }

    if exists.is_some() {
        let update_request = UpdateToolRequest {
            description: Some(item.description),
            endpoint_url: Some(item.endpoint_url),
            http_method: item.http_method,
            input_schema: item.input_schema,
            output_schema: item.output_schema,
            auth_header: item.auth_header,
            auth_secret: item.auth_secret,
            enabled: item.enabled,
            change_note: Some("imported".to_string()),
            scope_visibility: item.scope_visibility,
            rate_limit_per_minute: item.rate_limit_per_minute,
            circuit_breaker_threshold: item.circuit_breaker_threshold,
            circuit_breaker_cooldown_seconds: item.circuit_breaker_cooldown_seconds,
        };
        let _ = update_tool(
            State(state.clone()),
            Extension(auth.clone()),
            Path((workspace_id, item.name.clone())),
            Json(update_request),
        )
        .await?;
        Ok(Some(false))
    } else {
        let create_request = CreateToolRequest {
            name: item.name.clone(),
            description: item.description,
            endpoint_url: item.endpoint_url,
            http_method: item.http_method,
            input_schema: item.input_schema,
            output_schema: item.output_schema,
            auth_header: item.auth_header,
            auth_secret: item.auth_secret,
            change_note: Some("imported".to_string()),
            scope_visibility: item.scope_visibility,
            rate_limit_per_minute: item.rate_limit_per_minute,
            circuit_breaker_threshold: item.circuit_breaker_threshold,
            circuit_breaker_cooldown_seconds: item.circuit_breaker_cooldown_seconds,
        };
        let _ = create_tool(
            State(state.clone()),
            Extension(auth.clone()),
            Path(workspace_id),
            Json(create_request),
        )
        .await?;
        Ok(Some(true))
    }
}

pub async fn invoke_tool_core(
    state: &AppState,
    workspace_id: Uuid,
    name: &str,
    body: Option<&Value>,
    headers: Option<&HashMap<String, String>>,
    source: SkillInvocationSource,
    actor: &str,
    version: Option<i32>,
) -> AppResult<(ToolTestResponse, i32)> {
    let (response, version) = invoke_workspace_skill(
        state,
        workspace_id,
        name,
        body,
        headers,
        source,
        actor,
        version,
    )
    .await?;
    Ok((
        ToolTestResponse {
            status: response.status,
            latency_ms: response.latency_ms,
            body: response.body,
        },
        version,
    ))
}

async fn enforce_change_note_for_compliance(
    state: &AppState,
    workspace_id: Uuid,
    change_note: Option<&str>,
) -> AppResult<()> {
    let config = WorkspaceConfigService::new(state.db.clone())
        .load(workspace_id)
        .await?;
    if config.compliance_mode
        && change_note
            .map(|note| note.trim().is_empty())
            .unwrap_or(true)
    {
        return Err(AppError::Validation(
            "change_note is required when compliance_mode is enabled".to_owned(),
        ));
    }
    Ok(())
}

fn map_tool_insert_error(error: sqlx::Error) -> AppError {
    if let sqlx::Error::Database(database_error) = &error {
        if database_error.is_unique_violation() {
            return AppError::Conflict("tool name already exists in workspace".to_owned());
        }
    }
    AppError::Database(error)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]
    use super::*;

    #[test]
    fn http_method_as_str_roundtrips_all_variants() {
        assert_eq!(HttpMethod::Get.as_str(), "GET");
        assert_eq!(HttpMethod::Post.as_str(), "POST");
        assert_eq!(HttpMethod::Put.as_str(), "PUT");
    }

    #[test]
    fn rollback_request_accepts_empty_body() {
        let parsed: RollbackToolRequest = serde_json::from_str("{}").expect("empty object");
        assert!(parsed.change_note.is_none());
    }

    #[test]
    fn rollback_request_parses_change_note() {
        let parsed: RollbackToolRequest =
            serde_json::from_str(r#"{"change_note":"revert bad URL"}"#).expect("body");
        assert_eq!(parsed.change_note.as_deref(), Some("revert bad URL"));
    }

    #[test]
    fn create_request_change_note_is_optional() {
        let body = r#"{"name":"x","description":"y","endpoint_url":"https://e.example/x"}"#;
        let parsed: CreateToolRequest = serde_json::from_str(body).expect("body");
        assert!(parsed.change_note.is_none());
    }

    #[test]
    fn tool_test_response_encodes_502_on_connection_error() {
        let body = serde_json::json!({ "error": "connection refused" });
        let resp = ToolTestResponse {
            status: 502,
            latency_ms: 5,
            body,
        };
        let encoded = match serde_json::to_value(&resp) {
            Ok(encoded) => encoded,
            Err(error) => panic!("ToolTestResponse should serialize: {error}"),
        };
        assert_eq!(encoded["status"], 502);
        assert!(encoded["body"]["error"]
            .as_str()
            .is_some_and(|s| s.contains("connection refused")));
    }

    // ── SSRF validation tests ────────────────────────────────────────────────

    #[test]
    fn validate_url_rejects_http_scheme() {
        assert!(validate_endpoint_url("http://example.com/api").is_err());
    }

    #[test]
    fn validate_url_rejects_non_url() {
        assert!(validate_endpoint_url("not-a-url").is_err());
    }

    #[test]
    fn validate_url_rejects_credentials() {
        assert!(validate_endpoint_url("https://user:pass@example.com/api").is_err());
        assert!(validate_endpoint_url("https://user@example.com/api").is_err());
    }

    #[test]
    fn validate_url_rejects_localhost() {
        assert!(validate_endpoint_url("https://localhost/api").is_err());
        assert!(validate_endpoint_url("https://localhost:8080/api").is_err());
        assert!(validate_endpoint_url("https://foo.localhost/api").is_err());
    }

    #[test]
    fn validate_url_rejects_loopback_ipv4() {
        assert!(validate_endpoint_url("https://127.0.0.1/api").is_err());
        assert!(validate_endpoint_url("https://127.1.2.3/api").is_err());
    }

    #[test]
    fn validate_url_rejects_loopback_ipv6() {
        assert!(validate_endpoint_url("https://[::1]/api").is_err());
    }

    #[test]
    fn validate_url_rejects_private_ipv4_10_block() {
        assert!(validate_endpoint_url("https://10.0.0.1/api").is_err());
        assert!(validate_endpoint_url("https://10.255.255.255/api").is_err());
    }

    #[test]
    fn validate_url_rejects_private_ipv4_172_block() {
        assert!(validate_endpoint_url("https://172.16.0.1/api").is_err());
        assert!(validate_endpoint_url("https://172.31.255.255/api").is_err());
    }

    #[test]
    fn validate_url_rejects_private_ipv4_192_168_block() {
        assert!(validate_endpoint_url("https://192.168.1.1/api").is_err());
    }

    #[test]
    fn validate_url_rejects_link_local_metadata() {
        // AWS/GCP/Azure instance metadata endpoint
        assert!(validate_endpoint_url("https://169.254.169.254/latest/meta-data/").is_err());
        // Generic link-local
        assert!(validate_endpoint_url("https://169.254.0.1/").is_err());
    }

    #[test]
    fn validate_url_rejects_ipv4_mapped_ipv6_private() {
        // ::ffff:10.0.0.1 — IPv4-mapped private
        assert!(validate_endpoint_url("https://[::ffff:10.0.0.1]/api").is_err());
        // ::ffff:127.0.0.1 — IPv4-mapped loopback
        assert!(validate_endpoint_url("https://[::ffff:127.0.0.1]/api").is_err());
    }

    #[test]
    fn validate_url_accepts_public_https() {
        // No real network call — just validates the syntax and IP-range checks pass.
        assert!(validate_endpoint_url("https://api.example.com/v1/hook").is_ok());
        assert!(validate_endpoint_url("https://hooks.example.org/callback").is_ok());
    }
}
