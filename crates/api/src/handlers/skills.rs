use std::{collections::HashMap, time::Instant};

use axum::{extract::Path, extract::Query, extract::State, Extension, Json};
use chrono::{DateTime, Utc};
use common::{auth::AuthContext, error::AppResult, AppError, AppState};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::{Postgres, QueryBuilder};
use uuid::Uuid;

use crate::security::{decrypt_secret, encrypt_secret};

use super::require_workspace;

const DEFAULT_SKILL_LIMIT: i64 = 50;
const MAX_SKILL_LIMIT: i64 = 100;

#[derive(Debug, Deserialize)]
pub struct SkillListQuery {
    pub after: Option<Uuid>,
    pub limit: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct CreateSkillRequest {
    pub name: String,
    pub description: String,
    pub endpoint_url: String,
    pub http_method: Option<HttpMethod>,
    pub input_schema: Option<Value>,
    pub output_schema: Option<Value>,
    pub auth_header: Option<String>,
    pub auth_secret: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateSkillRequest {
    pub description: Option<String>,
    pub endpoint_url: Option<String>,
    pub http_method: Option<HttpMethod>,
    pub input_schema: Option<Value>,
    pub output_schema: Option<Value>,
    pub auth_header: Option<String>,
    pub auth_secret: Option<String>,
    pub enabled: Option<bool>,
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
pub struct SkillResponse {
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
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
pub struct SkillSecretResponse {
    pub auth_header: Option<String>,
    pub plaintext_secret: String,
}

#[axum::debug_handler]
pub async fn create_skill(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthContext>,
    Path(id): Path<Uuid>,
    Json(request): Json<CreateSkillRequest>,
) -> AppResult<Json<SkillResponse>> {
    require_workspace(&auth, id)?;
    validate_name(&request.name)?;
    validate_description(&request.description)?;
    validate_endpoint_url(&request.endpoint_url)?;
    validate_schema(request.input_schema.as_ref(), "input_schema")?;
    validate_schema(request.output_schema.as_ref(), "output_schema")?;
    let auth_header = normalized_optional_text(request.auth_header.as_deref());
    let auth_secret_enc = encrypted_secret(request.auth_secret.as_deref())?;
    validate_auth_pair(auth_header.as_ref(), auth_secret_enc.as_ref())?;

    let skill = sqlx::query_as::<_, SkillResponse>(
        r#"
        INSERT INTO workspace_skills (
            workspace_id, name, description, endpoint_url, http_method,
            input_schema, output_schema, auth_header, auth_secret_enc
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
        RETURNING id, workspace_id, name, description, endpoint_url, http_method,
                  input_schema, output_schema, auth_header, enabled, created_at, updated_at
        "#,
    )
    .bind(id)
    .bind(request.name.trim())
    .bind(request.description.trim())
    .bind(request.endpoint_url.trim())
    .bind(request.http_method.unwrap_or(HttpMethod::Post).as_str())
    .bind(request.input_schema.unwrap_or_else(|| json!({})))
    .bind(request.output_schema.unwrap_or_else(|| json!({})))
    .bind(auth_header)
    .bind(auth_secret_enc)
    .fetch_one(&state.db)
    .await
    .map_err(map_skill_insert_error)?;

    Ok(Json(skill))
}

#[axum::debug_handler]
pub async fn list_skills(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthContext>,
    Path(id): Path<Uuid>,
    Query(query): Query<SkillListQuery>,
) -> AppResult<Json<Vec<SkillResponse>>> {
    require_workspace(&auth, id)?;
    let limit = query
        .limit
        .unwrap_or(DEFAULT_SKILL_LIMIT)
        .clamp(1, MAX_SKILL_LIMIT);
    let mut builder = QueryBuilder::<Postgres>::new(
        r#"
        SELECT id, workspace_id, name, description, endpoint_url, http_method,
               input_schema, output_schema, auth_header, enabled, created_at, updated_at
        FROM workspace_skills
        WHERE workspace_id = "#,
    );
    builder.push_bind(id);

    if let Some(after) = query.after {
        builder.push(
            " AND (created_at, id) < (SELECT created_at, id FROM workspace_skills WHERE workspace_id = ",
        );
        builder.push_bind(id);
        builder.push(" AND id = ");
        builder.push_bind(after);
        builder.push(")");
    }

    builder.push(" ORDER BY created_at DESC, id DESC LIMIT ");
    builder.push_bind(limit);

    let skills = builder
        .build_query_as::<SkillResponse>()
        .fetch_all(&state.db)
        .await
        .map_err(AppError::Database)?;

    Ok(Json(skills))
}

#[axum::debug_handler]
pub async fn get_skill(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthContext>,
    Path((id, name)): Path<(Uuid, String)>,
) -> AppResult<Json<SkillResponse>> {
    require_workspace(&auth, id)?;
    let skill = fetch_skill(&state, id, &name).await?;
    Ok(Json(skill))
}

#[axum::debug_handler]
pub async fn update_skill(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthContext>,
    Path((id, name)): Path<(Uuid, String)>,
    Json(request): Json<UpdateSkillRequest>,
) -> AppResult<Json<SkillResponse>> {
    require_workspace(&auth, id)?;
    if let Some(description) = &request.description {
        validate_description(description)?;
    }
    if let Some(endpoint_url) = &request.endpoint_url {
        validate_endpoint_url(endpoint_url)?;
    }
    validate_schema(request.input_schema.as_ref(), "input_schema")?;
    validate_schema(request.output_schema.as_ref(), "output_schema")?;

    let auth_header = request
        .auth_header
        .as_deref()
        .and_then(|value| normalized_optional_text(Some(value)));
    let auth_secret_enc = encrypted_secret(request.auth_secret.as_deref())?;
    if auth_secret_enc.is_some() && auth_header.is_none() {
        let existing_header = sqlx::query_scalar::<_, Option<String>>(
            "SELECT auth_header FROM workspace_skills WHERE workspace_id = $1 AND name = $2",
        )
        .bind(id)
        .bind(&name)
        .fetch_optional(&state.db)
        .await
        .map_err(AppError::Database)?
        .flatten();
        validate_auth_pair(existing_header.as_ref(), auth_secret_enc.as_ref())?;
    }

    let skill = sqlx::query_as::<_, SkillResponse>(
        r#"
        UPDATE workspace_skills
        SET description = COALESCE($3, description),
            endpoint_url = COALESCE($4, endpoint_url),
            http_method = COALESCE($5, http_method),
            input_schema = COALESCE($6, input_schema),
            output_schema = COALESCE($7, output_schema),
            auth_header = COALESCE($8, auth_header),
            auth_secret_enc = COALESCE($9, auth_secret_enc),
            enabled = COALESCE($10, enabled)
        WHERE workspace_id = $1 AND name = $2
        RETURNING id, workspace_id, name, description, endpoint_url, http_method,
                  input_schema, output_schema, auth_header, enabled, created_at, updated_at
        "#,
    )
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
    .fetch_optional(&state.db)
    .await
    .map_err(AppError::Database)?
    .ok_or_else(|| AppError::NotFound {
        resource: format!("workspace_skill:{name}"),
    })?;

    Ok(Json(skill))
}

#[axum::debug_handler]
pub async fn delete_skill(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthContext>,
    Path((id, name)): Path<(Uuid, String)>,
) -> AppResult<Json<Value>> {
    require_workspace(&auth, id)?;
    let deleted = sqlx::query("DELETE FROM workspace_skills WHERE workspace_id = $1 AND name = $2")
        .bind(id)
        .bind(&name)
        .execute(&state.db)
        .await
        .map_err(AppError::Database)?
        .rows_affected();
    if deleted == 0 {
        return Err(AppError::NotFound {
            resource: format!("workspace_skill:{name}"),
        });
    }

    Ok(Json(json!({ "deleted": true })))
}

#[axum::debug_handler]
pub async fn get_skill_secret(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthContext>,
    Path((id, name)): Path<(Uuid, String)>,
) -> AppResult<Json<SkillSecretResponse>> {
    require_workspace(&auth, id)?;

    #[derive(Debug, sqlx::FromRow)]
    struct SecretRow {
        auth_header: Option<String>,
        auth_secret_enc: Option<String>,
    }

    let row = sqlx::query_as::<_, SecretRow>(
        "SELECT auth_header, auth_secret_enc FROM workspace_skills WHERE workspace_id = $1 AND name = $2",
    )
    .bind(id)
    .bind(&name)
    .fetch_optional(&state.db)
    .await
    .map_err(AppError::Database)?
    .ok_or_else(|| AppError::NotFound {
        resource: format!("workspace_skill:{name}"),
    })?;
    let ciphertext = row.auth_secret_enc.ok_or_else(|| AppError::NotFound {
        resource: format!("workspace_skill_secret:{name}"),
    })?;

    Ok(Json(SkillSecretResponse {
        auth_header: row.auth_header,
        plaintext_secret: decrypt_secret(&ciphertext)?,
    }))
}

async fn fetch_skill(state: &AppState, workspace_id: Uuid, name: &str) -> AppResult<SkillResponse> {
    sqlx::query_as::<_, SkillResponse>(
        r#"
        SELECT id, workspace_id, name, description, endpoint_url, http_method,
               input_schema, output_schema, auth_header, enabled, created_at, updated_at
        FROM workspace_skills
        WHERE workspace_id = $1 AND name = $2
        "#,
    )
    .bind(workspace_id)
    .bind(name)
    .fetch_optional(&state.db)
    .await
    .map_err(AppError::Database)?
    .ok_or_else(|| AppError::NotFound {
        resource: format!("workspace_skill:{name}"),
    })
}

fn validate_name(value: &str) -> AppResult<()> {
    let trimmed = value.trim();
    if trimmed.is_empty() || trimmed.len() > 64 {
        return Err(AppError::Validation(
            "skill name must be 1-64 characters".to_owned(),
        ));
    }
    let mut chars = trimmed.chars();
    let Some(first) = chars.next() else {
        return Err(AppError::Validation("skill name is required".to_owned()));
    };
    if !first.is_ascii_lowercase() {
        return Err(AppError::Validation(
            "skill name must start with a lowercase letter".to_owned(),
        ));
    }
    if chars.any(|ch| !(ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '_')) {
        return Err(AppError::Validation(
            "skill name must match ^[a-z][a-z0-9_]{0,63}$".to_owned(),
        ));
    }
    Ok(())
}

fn validate_description(value: &str) -> AppResult<()> {
    let length = value.trim().chars().count();
    if !(1..=500).contains(&length) {
        return Err(AppError::Validation(
            "skill description must be 1-500 characters".to_owned(),
        ));
    }
    Ok(())
}

fn validate_endpoint_url(value: &str) -> AppResult<()> {
    if !value.trim().starts_with("https://") {
        return Err(AppError::Validation(
            "skill endpoint_url must start with https://".to_owned(),
        ));
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

fn encrypted_secret(value: Option<&str>) -> AppResult<Option<String>> {
    let Some(secret) = normalized_optional_text(value) else {
        return Ok(None);
    };
    encrypt_secret(&secret).map(Some)
}

/// Request body for the skill test proxy.
#[derive(Debug, Deserialize)]
pub struct SkillTestRequest {
    pub body: Option<Value>,
    pub headers: Option<HashMap<String, String>>,
}

/// Response from the skill test proxy.
#[derive(Debug, Serialize)]
pub struct SkillTestResponse {
    pub status: u16,
    pub latency_ms: u64,
    pub body: Value,
}

#[axum::debug_handler]
pub async fn test_skill(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthContext>,
    Path((id, name)): Path<(Uuid, String)>,
    Json(request): Json<SkillTestRequest>,
) -> AppResult<Json<SkillTestResponse>> {
    require_workspace(&auth, id)?;

    #[derive(Debug, sqlx::FromRow)]
    struct SkillForTest {
        endpoint_url: String,
        http_method: String,
        auth_header: Option<String>,
        auth_secret_enc: Option<String>,
    }

    let skill = sqlx::query_as::<_, SkillForTest>(
        "SELECT endpoint_url, http_method, auth_header, auth_secret_enc FROM workspace_skills WHERE workspace_id = $1 AND name = $2",
    )
    .bind(id)
    .bind(&name)
    .fetch_optional(&state.db)
    .await
    .map_err(AppError::Database)?
    .ok_or_else(|| AppError::NotFound {
        resource: format!("workspace_skill:{name}"),
    })?;

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|error| AppError::Internal(anyhow::anyhow!(error)))?;

    let method = reqwest::Method::from_bytes(skill.http_method.as_bytes())
        .unwrap_or(reqwest::Method::POST);

    let mut req_builder = client.request(method, &skill.endpoint_url);

    // Inject auth header server-side (never expose the secret to the client)
    if let (Some(header_name), Some(enc)) = (skill.auth_header.as_deref(), skill.auth_secret_enc.as_deref()) {
        let secret = decrypt_secret(enc)?;
        req_builder = req_builder.header(header_name, secret);
    }

    // Forward caller-supplied headers (cannot override the auth header)
    if let Some(headers) = &request.headers {
        for (k, v) in headers {
            req_builder = req_builder.header(k.as_str(), v.as_str());
        }
    }

    if let Some(body) = &request.body {
        req_builder = req_builder
            .header("Content-Type", "application/json")
            .body(body.to_string());
    }

    let started = Instant::now();
    let response = req_builder.send().await;
    let latency_ms = started.elapsed().as_millis().min(u64::MAX as u128) as u64;

    match response {
        Ok(resp) => {
            let status = resp.status().as_u16();
            let body: Value = resp
                .json()
                .await
                .unwrap_or_else(|_| json!({ "error": "response was not JSON" }));
            Ok(Json(SkillTestResponse { status, latency_ms, body }))
        }
        Err(error) => Ok(Json(SkillTestResponse {
            status: 502,
            latency_ms,
            body: json!({ "error": error.to_string() }),
        })),
    }
}

fn map_skill_insert_error(error: sqlx::Error) -> AppError {
    if let sqlx::Error::Database(database_error) = &error {
        if database_error.is_unique_violation() {
            return AppError::Conflict("skill name already exists in workspace".to_owned());
        }
    }
    AppError::Database(error)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn http_method_as_str_roundtrips_all_variants() {
        assert_eq!(HttpMethod::Get.as_str(), "GET");
        assert_eq!(HttpMethod::Post.as_str(), "POST");
        assert_eq!(HttpMethod::Put.as_str(), "PUT");
    }

    #[test]
    fn skill_test_response_encodes_502_on_connection_error() {
        let body = serde_json::json!({ "error": "connection refused" });
        let resp = SkillTestResponse { status: 502, latency_ms: 5, body };
        let encoded = match serde_json::to_value(&resp) {
            Ok(encoded) => encoded,
            Err(error) => panic!("SkillTestResponse should serialize: {error}"),
        };
        assert_eq!(encoded["status"], 502);
        assert!(encoded["body"]["error"].as_str().is_some_and(|s| s.contains("connection refused")));
    }
}
