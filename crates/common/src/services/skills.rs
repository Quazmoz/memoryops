use std::{collections::HashMap, net::IpAddr, time::Instant};

use anyhow::anyhow;
use reqwest::Url;
use serde::Serialize;
use serde_json::{json, Value};
use sqlx::PgPool;
use uuid::Uuid;

use crate::{
    audit::spawn_audit_log,
    crypto::{decrypt_secret_legacy_or_current, DecryptedSecret},
    error::AppResult,
    models::AuditAction,
    AppError, AppState,
};

const MAX_SKILL_RESPONSE_BYTES: usize = 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkillInvocationSource {
    Http,
    Mcp,
    Test,
}

impl SkillInvocationSource {
    fn as_str(self) -> &'static str {
        match self {
            SkillInvocationSource::Http => "http",
            SkillInvocationSource::Mcp => "mcp",
            SkillInvocationSource::Test => "test",
        }
    }
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct SkillInvocationResponse {
    pub status: u16,
    pub latency_ms: u64,
    pub body: Value,
}

#[derive(Debug, sqlx::FromRow)]
struct InvokeSkillRow {
    id: Uuid,
    endpoint_url: String,
    http_method: String,
    auth_header: Option<String>,
    auth_secret_enc: Option<String>,
    enabled: bool,
    version: i32,
    scope_visibility: String,
    rate_limit_per_minute: i32,
    circuit_breaker_threshold: i32,
    circuit_breaker_cooldown_seconds: i32,
}

pub async fn invoke_workspace_skill(
    state: &AppState,
    workspace_id: Uuid,
    name: &str,
    body: Option<&Value>,
    headers: Option<&HashMap<String, String>>,
    source: SkillInvocationSource,
    actor: &str,
    version: Option<i32>,
) -> AppResult<(SkillInvocationResponse, i32)> {
    let (query, has_version) = if let Some(v) = version {
        (
            sqlx::query_as::<_, InvokeSkillRow>(
                r#"
                SELECT t.id, v.endpoint_url, v.http_method, v.auth_header, v.auth_secret_enc, v.enabled,
                       v.version, v.scope_visibility, t.rate_limit_per_minute,
                       t.circuit_breaker_threshold, t.circuit_breaker_cooldown_seconds
                FROM workspace_tools t
                JOIN workspace_tool_versions v ON t.id = v.tool_id
                WHERE t.workspace_id = $1 AND t.name = $2 AND v.version = $3
                "#,
            )
            .bind(workspace_id)
            .bind(name)
            .bind(v),
            true,
        )
    } else {
        (
            sqlx::query_as::<_, InvokeSkillRow>(
                r#"
                SELECT id, endpoint_url, http_method, auth_header, auth_secret_enc, enabled,
                       version, scope_visibility, rate_limit_per_minute,
                       circuit_breaker_threshold, circuit_breaker_cooldown_seconds
                FROM workspace_tools
                WHERE workspace_id = $1 AND name = $2
                "#,
            )
            .bind(workspace_id)
            .bind(name),
            false,
        )
    };

    let skill = query
        .fetch_optional(&state.db)
        .await
        .map_err(AppError::Database)?
        .ok_or_else(|| {
            if has_version {
                AppError::NotFound {
                    resource: format!("workspace_tool:{name}@{}", version.unwrap()),
                }
            } else {
                AppError::NotFound {
                    resource: format!("workspace_tool:{name}"),
                }
            }
        })?;

    if !skill.enabled {
        return Err(AppError::Validation(format!(
            "tool {name} is disabled"
        )));
    }

    if matches!(source, SkillInvocationSource::Mcp) && skill.scope_visibility == "private" {
        return Err(AppError::Forbidden);
    }

    if skill.rate_limit_per_minute > 0 {
        let count: i64 = sqlx::query_scalar(
            r#"
            SELECT COUNT(*) FROM workspace_tool_invocations
            WHERE tool_id = $1 AND occurred_at > NOW() - INTERVAL '60 seconds'
            "#,
        )
        .bind(skill.id)
        .fetch_one(&state.db)
        .await
        .map_err(AppError::Database)?;
        if count >= skill.rate_limit_per_minute as i64 {
            return Err(AppError::RateLimited {
                retry_after_secs: 60,
            });
        }
    }

    if skill.circuit_breaker_threshold > 0 {
        let failures: i64 = sqlx::query_scalar(
            r#"
            SELECT COUNT(*) FROM workspace_tool_invocations
            WHERE tool_id = $1
              AND occurred_at > NOW() - make_interval(secs => $2)
              AND (status_code < 200 OR status_code >= 300)
            "#,
        )
        .bind(skill.id)
        .bind(skill.circuit_breaker_cooldown_seconds as f64)
        .fetch_one(&state.db)
        .await
        .map_err(AppError::Database)?;
        if failures >= skill.circuit_breaker_threshold as i64 {
            return Err(AppError::Conflict(format!(
                "tool {name} circuit breaker open ({} failures in last {}s)",
                failures, skill.circuit_breaker_cooldown_seconds
            )));
        }
    }

    validate_endpoint_url_dns(&skill.endpoint_url, state.config.server.allow_private_ips).await?;

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|error| AppError::Internal(anyhow!(error)))?;

    let method = reqwest::Method::from_bytes(skill.http_method.as_bytes())
        .unwrap_or(reqwest::Method::POST);
    let mut req_builder = client.request(method, &skill.endpoint_url);

    if let (Some(header_name), Some(enc)) = (
        skill.auth_header.as_deref(),
        skill.auth_secret_enc.as_deref(),
    ) {
        let decrypted =
            decrypt_secret_legacy_or_current(state.app_secret_key.as_ref().as_str(), enc)?;
        persist_migrated_ciphertext(&state.db, workspace_id, name, &decrypted).await?;
        req_builder = req_builder.header(header_name, decrypted.plaintext);
    }

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
    if let Some(caller_headers) = headers {
        let auth_header_lower = skill
            .auth_header
            .as_deref()
            .map(|header| header.to_ascii_lowercase())
            .unwrap_or_default();
        for (key, value) in caller_headers {
            let normalized = key.to_ascii_lowercase();
            if BLOCKED_REQUEST_HEADERS.contains(&normalized.as_str()) {
                continue;
            }
            if !auth_header_lower.is_empty() && normalized == auth_header_lower {
                continue;
            }
            req_builder = req_builder.header(key.as_str(), value.as_str());
        }
    }

    if let Some(body) = body {
        req_builder = req_builder
            .header("Content-Type", "application/json")
            .body(body.to_string());
    }

    let started = Instant::now();
    let response = req_builder.send().await;
    let latency_ms = started.elapsed().as_millis().min(u64::MAX as u128) as u64;

    let (status, body_value, error_text) = match response {
        Ok(resp) => {
            let status = resp.status().as_u16();
            let body = read_skill_response_body(resp).await;
            (status, body, None)
        }
        Err(error) => (
            502u16,
            json!({ "error": error.to_string() }),
            Some(error.to_string()),
        ),
    };

    let _ = sqlx::query(
        r#"
        INSERT INTO workspace_tool_invocations (
            tool_id, workspace_id, tool_name, tool_version, actor,
            source, status_code, latency_ms, error
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
        "#,
    )
    .bind(skill.id)
    .bind(workspace_id)
    .bind(name)
    .bind(skill.version)
    .bind(actor)
    .bind(source.as_str())
    .bind(status as i32)
    .bind(latency_ms.min(i32::MAX as u64) as i32)
    .bind(error_text)
    .execute(&state.db)
    .await
    .map_err(AppError::Database);

    spawn_audit_log(
        state.db.clone(),
        workspace_id,
        actor.to_owned(),
        AuditAction::ToolInvoked,
        skill.id,
        "workspace_tool",
        Some(json!({
            "name": name,
            "version": skill.version,
            "source": source.as_str(),
            "status": status,
            "latency_ms": latency_ms,
        })),
    );

    Ok((
        SkillInvocationResponse {
            status,
            latency_ms,
            body: body_value,
        },
        skill.version,
    ))
}

async fn persist_migrated_ciphertext(
    db: &PgPool,
    workspace_id: Uuid,
    name: &str,
    decrypted: &DecryptedSecret,
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

async fn validate_endpoint_url_dns(url: &str, allow_private_ips: bool) -> AppResult<()> {
    if allow_private_ips {
        return Ok(());
    }

    let parsed = Url::parse(url)
        .map_err(|_| AppError::Validation("invalid endpoint URL".to_owned()))?;

    let host = parsed.host_str().unwrap_or("");
    let ip_str = if host.starts_with('[') && host.ends_with(']') {
        &host[1..host.len() - 1]
    } else {
        host
    };
    if let Ok(ip) = ip_str.parse::<IpAddr>() {
        reject_forbidden_ip(ip)?;
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

fn reject_forbidden_ip(ip: IpAddr) -> AppResult<()> {
    match ip {
        IpAddr::V4(v4) => {
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
        IpAddr::V6(v6) => {
            if v6.is_loopback() || v6.is_multicast() || v6.is_unspecified() {
                return Err(AppError::Validation(
                    "tool endpoint_url resolves to a forbidden address".to_owned(),
                ));
            }
            if let Some(v4) = v6.to_ipv4() {
                reject_forbidden_ip(IpAddr::V4(v4))?;
            }
        }
    }
    Ok(())
}

async fn read_skill_response_body(mut response: reqwest::Response) -> Value {
    let mut bytes = Vec::new();

    loop {
        let chunk = match response.chunk().await {
            Ok(chunk) => chunk,
            Err(error) => {
                return json!({ "error": format!("failed to read response body: {error}") });
            }
        };
        let Some(chunk) = chunk else {
            break;
        };

        if bytes.len().saturating_add(chunk.len()) > MAX_SKILL_RESPONSE_BYTES {
            return json!({
                "error": "response body exceeded size limit",
                "limit_bytes": MAX_SKILL_RESPONSE_BYTES
            });
        }
        bytes.extend_from_slice(&chunk);
    }

    serde_json::from_slice::<Value>(&bytes)
        .unwrap_or_else(|_| json!({ "error": "response was not JSON" }))
}
