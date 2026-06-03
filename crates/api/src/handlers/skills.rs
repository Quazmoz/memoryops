use std::{collections::HashMap, time::Instant};

use axum::{extract::Path, extract::Query, extract::State, Extension, Json};
use chrono::{DateTime, Utc};
use common::{auth::AuthContext, error::AppResult, AppError, AppState};
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::{PgPool, Postgres, QueryBuilder};
use uuid::Uuid;

use crate::security::{decrypt_secret_legacy_or_current, encrypt_secret};

use super::require_workspace;

const DEFAULT_SKILL_LIMIT: i64 = 50;
const MAX_SKILL_LIMIT: i64 = 100;
const MAX_SKILL_TEST_RESPONSE_BYTES: usize = 1024 * 1024;

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
    validate_endpoint_url_dns(&request.endpoint_url).await?;
    validate_schema(request.input_schema.as_ref(), "input_schema")?;
    validate_schema(request.output_schema.as_ref(), "output_schema")?;
    let auth_header = normalized_optional_text(request.auth_header.as_deref());
    let auth_secret_enc = encrypted_secret(&state, request.auth_secret.as_deref())?;
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
        validate_endpoint_url_dns(endpoint_url).await?;
    }
    validate_schema(request.input_schema.as_ref(), "input_schema")?;
    validate_schema(request.output_schema.as_ref(), "output_schema")?;

    let auth_header = request
        .auth_header
        .as_deref()
        .and_then(|value| normalized_optional_text(Some(value)));
    let auth_secret_enc = encrypted_secret(&state, request.auth_secret.as_deref())?;
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
    let decrypted =
        decrypt_secret_legacy_or_current(state.app_secret_key.as_ref().as_str(), &ciphertext)?;
    persist_migrated_ciphertext(&state.db, id, &name, &decrypted).await?;

    Ok(Json(SkillSecretResponse {
        auth_header: row.auth_header,
        plaintext_secret: decrypted.plaintext,
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
    let url = reqwest::Url::parse(value.trim())
        .map_err(|_| AppError::Validation("skill endpoint_url is not a valid URL".to_owned()))?;

    if url.scheme() != "https" {
        return Err(AppError::Validation(
            "skill endpoint_url must use the https scheme".to_owned(),
        ));
    }

    if !url.username().is_empty() || url.password().is_some() {
        return Err(AppError::Validation(
            "skill endpoint_url must not contain credentials".to_owned(),
        ));
    }

    let host = url
        .host_str()
        .ok_or_else(|| AppError::Validation("skill endpoint_url must have a host".to_owned()))?;

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
            "skill endpoint_url host is not permitted".to_owned(),
        ));
    }

    // Parse as an IP literal (IPv4 or bracket-stripped IPv6)
    if let Ok(ip) = lower.parse::<std::net::IpAddr>() {
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
                    "skill endpoint_url resolves to a forbidden address".to_owned(),
                ));
            }
        }
        std::net::IpAddr::V6(v6) => {
            if v6.is_loopback() || v6.is_multicast() || v6.is_unspecified() {
                return Err(AppError::Validation(
                    "skill endpoint_url resolves to a forbidden address".to_owned(),
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
        .map_err(|_| AppError::Validation("skill endpoint_url could not be resolved".to_owned()))?;

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
        "UPDATE workspace_skills SET auth_secret_enc = $3 WHERE workspace_id = $1 AND name = $2",
    )
    .bind(workspace_id)
    .bind(name)
    .bind(migrated_ciphertext)
    .execute(db)
    .await
    .map_err(AppError::Database)?;

    Ok(())
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

    // Validate the stored endpoint URL at call time (DNS rebinding defence)
    validate_endpoint_url_dns(&skill.endpoint_url).await?;

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|error| AppError::Internal(anyhow::anyhow!(error)))?;

    let method =
        reqwest::Method::from_bytes(skill.http_method.as_bytes()).unwrap_or(reqwest::Method::POST);

    let mut req_builder = client.request(method, &skill.endpoint_url);

    // Inject auth header server-side (never expose the secret to the client)
    if let (Some(header_name), Some(enc)) = (
        skill.auth_header.as_deref(),
        skill.auth_secret_enc.as_deref(),
    ) {
        let decrypted =
            decrypt_secret_legacy_or_current(state.app_secret_key.as_ref().as_str(), enc)?;
        persist_migrated_ciphertext(&state.db, id, &name, &decrypted).await?;
        req_builder = req_builder.header(header_name, decrypted.plaintext);
    }

    // Forward a safe subset of caller-supplied headers.
    // Headers that could override auth, change routing, or affect connection
    // management are blocked regardless of what the caller sends.
    const BLOCKED_REQUEST_HEADERS: &[&str] = &[
        "authorization",
        "proxy-authorization",
        "cookie",
        "set-cookie",
        "host",
        "content-length",
        "transfer-encoding",
        "connection",
        "upgrade",
        "te",
        "expect",
        "x-forwarded-for",
        "x-real-ip",
    ];
    if let Some(caller_headers) = &request.headers {
        let auth_header_lower = skill
            .auth_header
            .as_deref()
            .map(|h| h.to_ascii_lowercase())
            .unwrap_or_default();
        for (k, v) in caller_headers {
            let normalized = k.to_ascii_lowercase();
            if BLOCKED_REQUEST_HEADERS.contains(&normalized.as_str()) {
                continue;
            }
            // Also block whatever header name the skill uses for auth
            if !auth_header_lower.is_empty() && normalized == auth_header_lower {
                continue;
            }
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
            let body = read_skill_test_body(resp).await;
            Ok(Json(SkillTestResponse {
                status,
                latency_ms,
                body,
            }))
        }
        Err(error) => Ok(Json(SkillTestResponse {
            status: 502,
            latency_ms,
            body: json!({ "error": error.to_string() }),
        })),
    }
}

async fn read_skill_test_body(response: reqwest::Response) -> Value {
    let mut stream = response.bytes_stream();
    let mut bytes = Vec::new();

    while let Some(chunk) = stream.next().await {
        let chunk = match chunk {
            Ok(chunk) => chunk,
            Err(error) => {
                return json!({ "error": format!("failed to read response body: {error}") })
            }
        };
        if bytes.len().saturating_add(chunk.len()) > MAX_SKILL_TEST_RESPONSE_BYTES {
            return json!({
                "error": "response body exceeded size limit",
                "limit_bytes": MAX_SKILL_TEST_RESPONSE_BYTES
            });
        }
        bytes.extend_from_slice(&chunk);
    }

    serde_json::from_slice::<Value>(&bytes)
        .unwrap_or_else(|_| json!({ "error": "response was not JSON" }))
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
        let resp = SkillTestResponse {
            status: 502,
            latency_ms: 5,
            body,
        };
        let encoded = match serde_json::to_value(&resp) {
            Ok(encoded) => encoded,
            Err(error) => panic!("SkillTestResponse should serialize: {error}"),
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
