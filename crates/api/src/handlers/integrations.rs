use anyhow::anyhow;
use axum::{extract::Path, extract::Query, extract::State, http::StatusCode, Extension, Json};
use chrono::{DateTime, Utc};
use common::{
    audit::spawn_audit_log,
    auth::AuthContext,
    error::AppResult,
    models::{AuditAction, EventType, IntegrationStatus, Source},
    AppError, AppState,
};
use ingestion::{
    queue::{publish_raw_event_with_mode, PublishMode},
    store::{insert_raw_event, raw_event_needs_publish, NewRawEvent},
    STREAM_KEY,
};
use processor::{
    dlq::{dlq_key as dlq_entry_key, dlq_list_key},
    worker::PROCESSOR_JOBS_STREAM,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use uuid::Uuid;

use crate::security::{decrypt_secret_legacy_or_current, encrypt_secret};

use super::require_workspace;

const MAX_PAYLOAD_SUMMARY_CHARS: usize = 240;
const DEFAULT_DLQ_LIMIT: i64 = 100;
const MAX_DLQ_LIMIT: i64 = 500;
const DEFAULT_SYNC_LIMIT: usize = 25;
const MAX_SYNC_LIMIT: usize = 100;
const GITHUB_API_BASE: &str = "https://api.github.com";

#[derive(Debug, Deserialize)]
pub struct CreateIntegrationRequest {
    pub source: Source,
    #[serde(default)]
    pub webhook_secret: Option<String>,
    #[serde(default)]
    pub api_token: Option<String>,
    #[serde(default)]
    pub api_sync_enabled: Option<bool>,
    #[serde(default)]
    pub sync_config: Option<Value>,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct IntegrationResponse {
    pub source: Source,
    pub last_event_at: Option<DateTime<Utc>>,
    pub events_24h: i64,
    pub errors_24h: i64,
    pub status: IntegrationStatus,
    pub has_webhook_secret: bool,
    pub has_api_credential: bool,
    pub api_sync_enabled: bool,
    pub sync_config: Value,
    pub last_sync_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Serialize)]
pub struct DlqEntryResponse {
    pub job_id: Uuid,
    pub payload_summary: String,
    pub error: String,
    pub retry_count: u32,
    pub failed_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Deserialize)]
pub struct DlqQuery {
    pub limit: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct ConnectorSyncRequest {
    pub repo: Option<String>,
    pub since: Option<DateTime<Utc>>,
    pub limit: Option<usize>,
}

#[derive(Debug, Serialize)]
pub struct ConnectorSyncResponse {
    pub source: Source,
    pub queued_events: usize,
    pub skipped_events: usize,
    pub status: &'static str,
    pub message: String,
}

#[derive(Debug, Deserialize)]
struct StoredDlqEntry {
    pub job_id: Option<Uuid>,
    pub event_id: Option<Uuid>,
    pub memory_id: Option<Uuid>,
    pub payload: Option<Value>,
    pub error: Option<String>,
    pub retry_count: Option<u32>,
    pub failed_at: Option<DateTime<Utc>>,
}

#[axum::debug_handler]
#[tracing::instrument(skip(state, request))]
pub async fn create_integration(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthContext>,
    Path(id): Path<Uuid>,
    Json(request): Json<CreateIntegrationRequest>,
) -> AppResult<Json<IntegrationResponse>> {
    require_workspace(&auth, id)?;
    let webhook_secret = request
        .webhook_secret
        .as_deref()
        .map(str::trim)
        .filter(|secret| !secret.is_empty())
        .map(ToOwned::to_owned);
    let api_token = request
        .api_token
        .as_deref()
        .map(str::trim)
        .filter(|token| !token.is_empty())
        .map(ToOwned::to_owned);

    if webhook_secret.is_none() && api_token.is_none() {
        return Err(AppError::Validation(
            "webhook_secret or api_token is required".to_owned(),
        ));
    }

    let webhook_secret_enc = match webhook_secret.as_deref() {
        Some(secret) => Some(encrypt_secret(
            state.app_secret_key.as_ref().as_str(),
            secret,
        )?),
        None => None,
    };
    let api_token_enc = match api_token.as_deref() {
        Some(token) => Some(encrypt_secret(state.app_secret_key.as_ref().as_str(), token)?),
        None => None,
    };
    let api_sync_enabled = request
        .api_sync_enabled
        .unwrap_or_else(|| api_token_enc.is_some());
    let sync_config = request.sync_config.unwrap_or_else(|| json!({}));

    sqlx::query(
        r#"
        INSERT INTO integrations (
            workspace_id,
            source,
            webhook_secret_hash,
            webhook_secret_enc,
            api_token_enc,
            api_sync_enabled,
            sync_config,
            deleted_at
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, NULL)
        ON CONFLICT (workspace_id, source) DO UPDATE
        SET webhook_secret_hash = CASE
                WHEN EXCLUDED.webhook_secret_enc IS NOT NULL THEN EXCLUDED.webhook_secret_hash
                ELSE integrations.webhook_secret_hash
            END,
            webhook_secret_enc = COALESCE(EXCLUDED.webhook_secret_enc, integrations.webhook_secret_enc),
            api_token_enc = COALESCE(EXCLUDED.api_token_enc, integrations.api_token_enc),
            api_sync_enabled = EXCLUDED.api_sync_enabled,
            sync_config = EXCLUDED.sync_config,
            deleted_at = NULL
        "#,
    )
    .bind(id)
    .bind(request.source)
    .bind(Option::<&str>::None)
    .bind(webhook_secret_enc.as_deref())
    .bind(api_token_enc.as_deref())
    .bind(api_sync_enabled)
    .bind(&sync_config)
    .execute(&state.db)
    .await
    .map_err(AppError::Database)?;

    sqlx::query(
        r#"
        INSERT INTO integration_health (workspace_id, source)
        VALUES ($1, $2)
        ON CONFLICT (workspace_id, source) DO NOTHING
        "#,
    )
    .bind(id)
    .bind(request.source)
    .execute(&state.db)
    .await
    .map_err(AppError::Database)?;

    spawn_audit_log(
        state.db.clone(),
        id,
        auth.actor(),
        AuditAction::IntegrationAdded,
        id,
        "integration",
        Some(json!({
            "source": request.source,
            "has_webhook_secret": webhook_secret_enc.is_some(),
            "has_api_credential": api_token_enc.is_some(),
            "api_sync_enabled": api_sync_enabled,
        })),
    );

    let integration = get_integration(&state, id, request.source)
        .await?
        .ok_or_else(|| AppError::NotFound {
            resource: format!("integration:{:?}", request.source),
        })?;

    Ok(Json(integration))
}

#[axum::debug_handler]
pub async fn list_integrations(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthContext>,
    Path(id): Path<Uuid>,
) -> AppResult<Json<Vec<IntegrationResponse>>> {
    require_workspace(&auth, id)?;
    let integrations = sqlx::query_as::<_, IntegrationResponse>(
        integration_select_sql(
            r#"
        WHERE integrations.workspace_id = $1
          AND integrations.deleted_at IS NULL
        ORDER BY integrations.source::text ASC
        "#,
        ),
    )
    .bind(id)
    .fetch_all(&state.db)
    .await
    .map_err(AppError::Database)?;

    Ok(Json(integrations))
}

#[axum::debug_handler]
pub async fn delete_integration(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthContext>,
    Path((id, source)): Path<(Uuid, Source)>,
) -> AppResult<StatusCode> {
    require_workspace(&auth, id)?;
    let affected = sqlx::query(
        r#"
        UPDATE integrations
        SET deleted_at = now()
        WHERE workspace_id = $1 AND source = $2 AND deleted_at IS NULL
        "#,
    )
    .bind(id)
    .bind(source)
    .execute(&state.db)
    .await
    .map_err(AppError::Database)?
    .rows_affected();

    if affected == 0 {
        return Err(AppError::NotFound {
            resource: format!("integration:{source:?}"),
        });
    }

    spawn_audit_log(
        state.db.clone(),
        id,
        auth.actor(),
        AuditAction::IntegrationRemoved,
        id,
        "integration",
        Some(json!({ "source": source })),
    );

    Ok(StatusCode::NO_CONTENT)
}

#[axum::debug_handler]
pub async fn start_connector_sync(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthContext>,
    Path((id, source)): Path<(Uuid, Source)>,
    Json(request): Json<ConnectorSyncRequest>,
) -> AppResult<(StatusCode, Json<ConnectorSyncResponse>)> {
    require_workspace(&auth, id)?;
    let response = match source {
        Source::GitHub => sync_github_issues(&state, id, request).await?,
        Source::Slack | Source::Jira | Source::Linear | Source::Observation => {
            return Err(AppError::Validation(format!(
                "API sync adapter for {source:?} is not implemented yet"
            )));
        }
    };

    Ok((StatusCode::ACCEPTED, Json(response)))
}

#[axum::debug_handler]
pub async fn list_dlq(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthContext>,
    Path(id): Path<Uuid>,
    Query(query): Query<DlqQuery>,
) -> AppResult<Json<Vec<DlqEntryResponse>>> {
    require_workspace(&auth, id)?;
    let limit = query
        .limit
        .unwrap_or(DEFAULT_DLQ_LIMIT)
        .clamp(1, MAX_DLQ_LIMIT);
    let values = dlq_values(&state, id, limit).await?;
    let entries = values
        .iter()
        .filter_map(|raw| parse_dlq_response(raw))
        .collect::<Vec<_>>();

    Ok(Json(entries))
}

#[axum::debug_handler]
pub async fn retry_dlq(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthContext>,
    Path((id, job_id)): Path<(Uuid, Uuid)>,
) -> AppResult<StatusCode> {
    require_workspace(&auth, id)?;
    let raw = find_dlq_value(&state, id, job_id)
        .await?
        .ok_or_else(|| AppError::NotFound {
            resource: format!("dlq:{job_id}"),
        })?;
    let retry_target = parse_dlq_retry_target(&raw).ok_or_else(|| {
        AppError::Validation(
            "DLQ entry does not contain a retryable event_id or memory_id".to_owned(),
        )
    })?;
    let mut redis = state
        .redis
        .get()
        .await
        .map_err(|error| AppError::Internal(anyhow!(error)))?;
    let key = dlq_list_key(id);
    let entry_key = dlq_entry_key(id, job_id);

    let mut pipe = redis::pipe();
    pipe.atomic();
    match retry_target {
        DlqRetryTarget::RawEvent(event_id) => {
            pipe.cmd("XADD")
                .arg(STREAM_KEY)
                .arg("*")
                .arg("event_id")
                .arg(event_id.to_string())
                .arg("workspace_id")
                .arg(id.to_string());
        }
        DlqRetryTarget::SlowProcessorJob(memory_id) => {
            pipe.cmd("XADD")
                .arg(PROCESSOR_JOBS_STREAM)
                .arg("*")
                .arg("memory_id")
                .arg(memory_id.to_string())
                .arg("workspace_id")
                .arg(id.to_string())
                .arg("attempts")
                .arg("0");
        }
    }

    pipe.cmd("LREM")
        .arg(&key)
        .arg(1)
        .arg(&raw)
        .cmd("DEL")
        .arg(entry_key)
        .query_async::<(String, i64, i64)>(&mut *redis)
        .await
        .map_err(|error| AppError::Internal(anyhow!(error)))?;

    Ok(StatusCode::ACCEPTED)
}

#[axum::debug_handler]
pub async fn delete_dlq(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthContext>,
    Path((id, job_id)): Path<(Uuid, Uuid)>,
) -> AppResult<StatusCode> {
    require_workspace(&auth, id)?;
    let raw = find_dlq_value(&state, id, job_id)
        .await?
        .ok_or_else(|| AppError::NotFound {
            resource: format!("dlq:{job_id}"),
        })?;
    let mut redis = state
        .redis
        .get()
        .await
        .map_err(|error| AppError::Internal(anyhow!(error)))?;
    redis::pipe()
        .cmd("LREM")
        .arg(dlq_list_key(id))
        .arg(1)
        .arg(raw)
        .cmd("DEL")
        .arg(dlq_entry_key(id, job_id))
        .query_async::<(i64, i64)>(&mut *redis)
        .await
        .map_err(|error| AppError::Internal(anyhow!(error)))?;

    Ok(StatusCode::NO_CONTENT)
}

async fn sync_github_issues(
    state: &AppState,
    workspace_id: Uuid,
    request: ConnectorSyncRequest,
) -> AppResult<ConnectorSyncResponse> {
    let repo = normalize_github_repo(request.repo.as_deref())?;
    let limit = request.limit.unwrap_or(DEFAULT_SYNC_LIMIT).clamp(1, MAX_SYNC_LIMIT);
    let token = integration_api_token(state, workspace_id, Source::GitHub).await?;
    let issues = fetch_github_issues(&token, &repo, request.since, limit).await?;
    let mut redis = state
        .redis
        .get()
        .await
        .map_err(|error| AppError::Internal(anyhow!(error)))?;
    let mut queued_events = 0usize;
    let mut skipped_events = 0usize;

    for item in issues {
        let Some(number) = item.get("number").and_then(Value::as_i64) else {
            skipped_events += 1;
            continue;
        };
        let is_pull_request = item.get("pull_request").is_some();
        let event_type = if is_pull_request {
            EventType::PullRequest
        } else {
            EventType::Issue
        };
        let actor = item
            .pointer("/user/login")
            .and_then(Value::as_str)
            .unwrap_or("github-api-sync")
            .to_owned();
        let occurred_at = github_item_time(&item).unwrap_or_else(Utc::now);
        let updated_marker = item
            .get("updated_at")
            .and_then(Value::as_str)
            .unwrap_or("unknown");
        let payload = github_item_payload(&repo, &item, is_pull_request);
        let idempotency_key = format!(
            "github:api-sync:{workspace_id}:{repo}:{}:{number}:{updated_marker}",
            if is_pull_request { "pull_request" } else { "issue" }
        );

        let event = insert_raw_event(
            &state.db,
            &NewRawEvent {
                workspace_id,
                source: Source::GitHub,
                event_type,
                actor,
                payload,
                idempotency_key,
                occurred_at,
            },
        )
        .await?;

        if raw_event_needs_publish(&state.db, event.id).await? {
            publish_raw_event_with_mode(&mut *redis, &event, PublishMode::Strict).await?;
            queued_events += 1;
        } else {
            skipped_events += 1;
        }
    }

    update_sync_health(state, workspace_id, Source::GitHub, queued_events).await?;

    Ok(ConnectorSyncResponse {
        source: Source::GitHub,
        queued_events,
        skipped_events,
        status: "queued",
        message: format!(
            "Queued {queued_events} GitHub API events from {repo}; skipped {skipped_events}."
        ),
    })
}

async fn fetch_github_issues(
    token: &str,
    repo: &str,
    since: Option<DateTime<Utc>>,
    limit: usize,
) -> AppResult<Vec<Value>> {
    let mut url = format!(
        "{GITHUB_API_BASE}/repos/{repo}/issues?state=all&sort=updated&direction=desc&per_page={limit}"
    );
    if let Some(since) = since {
        url.push_str("&since=");
        url.push_str(&since.to_rfc3339());
    }

    let response = reqwest::Client::new()
        .get(url)
        .header("Accept", "application/vnd.github+json")
        .header("User-Agent", "MemoryOps/0.20")
        .bearer_auth(token)
        .send()
        .await
        .map_err(|error| AppError::Internal(anyhow!(error)))?;
    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        return Err(AppError::Validation(format!(
            "GitHub API sync failed with {status}: {}",
            truncate(&body, 300)
        )));
    }

    response
        .json::<Vec<Value>>()
        .await
        .map_err(|error| AppError::Internal(anyhow!(error)))
}

async fn integration_api_token(
    state: &AppState,
    workspace_id: Uuid,
    source: Source,
) -> AppResult<String> {
    let encrypted = sqlx::query_scalar::<_, Option<String>>(
        r#"
        SELECT api_token_enc
        FROM integrations
        WHERE workspace_id = $1
          AND source = $2
          AND deleted_at IS NULL
        LIMIT 1
        "#,
    )
    .bind(workspace_id)
    .bind(source)
    .fetch_optional(&state.db)
    .await
    .map_err(AppError::Database)?
    .flatten()
    .ok_or_else(|| AppError::Validation("API token is required before running API sync".to_owned()))?;

    let decrypted = decrypt_secret_legacy_or_current(state.app_secret_key.as_ref().as_str(), &encrypted)?;
    if let Some(migrated) = decrypted.migrated_ciphertext.as_deref() {
        sqlx::query(
            r#"
            UPDATE integrations
            SET api_token_enc = $3
            WHERE workspace_id = $1 AND source = $2
            "#,
        )
        .bind(workspace_id)
        .bind(source)
        .bind(migrated)
        .execute(&state.db)
        .await
        .map_err(AppError::Database)?;
    }

    Ok(decrypted.plaintext)
}

async fn update_sync_health(
    state: &AppState,
    workspace_id: Uuid,
    source: Source,
    queued_events: usize,
) -> AppResult<()> {
    sqlx::query(
        r#"
        UPDATE integrations
        SET last_sync_at = now(), api_sync_enabled = true
        WHERE workspace_id = $1 AND source = $2
        "#,
    )
    .bind(workspace_id)
    .bind(source)
    .execute(&state.db)
    .await
    .map_err(AppError::Database)?;

    sqlx::query(
        r#"
        INSERT INTO integration_health (workspace_id, source, last_event_at, events_24h, status)
        VALUES ($1, $2, now(), $3, 'active'::integration_status)
        ON CONFLICT (workspace_id, source) DO UPDATE
        SET last_event_at = CASE
                WHEN $3 > 0 THEN now()
                ELSE integration_health.last_event_at
            END,
            events_24h = integration_health.events_24h + $3,
            status = 'active'::integration_status
        "#,
    )
    .bind(workspace_id)
    .bind(source)
    .bind(queued_events as i64)
    .execute(&state.db)
    .await
    .map_err(AppError::Database)?;

    Ok(())
}

fn normalize_github_repo(repo: Option<&str>) -> AppResult<String> {
    let repo = repo
        .map(str::trim)
        .filter(|repo| !repo.is_empty())
        .ok_or_else(|| AppError::Validation("repo is required for GitHub API sync".to_owned()))?;
    let mut parts = repo.split('/');
    let owner = parts.next().unwrap_or_default();
    let name = parts.next().unwrap_or_default();
    if owner.is_empty() || name.is_empty() || parts.next().is_some() {
        return Err(AppError::Validation(
            "repo must use owner/name format".to_owned(),
        ));
    }
    if repo.chars().any(char::is_whitespace) {
        return Err(AppError::Validation(
            "repo must not contain whitespace".to_owned(),
        ));
    }
    Ok(repo.to_owned())
}

fn github_item_payload(repo: &str, item: &Value, is_pull_request: bool) -> Value {
    if is_pull_request {
        json!({
            "action": "api_sync",
            "pull_request": item,
            "repository": { "full_name": repo },
            "sync": { "source": "api", "resource": "issues", "adapter": "github" }
        })
    } else {
        json!({
            "action": "api_sync",
            "issue": item,
            "repository": { "full_name": repo },
            "sync": { "source": "api", "resource": "issues", "adapter": "github" }
        })
    }
}

fn github_item_time(item: &Value) -> Option<DateTime<Utc>> {
    item.get("updated_at")
        .or_else(|| item.get("created_at"))
        .and_then(Value::as_str)
        .and_then(|value| DateTime::parse_from_rfc3339(value).ok())
        .map(|value| value.with_timezone(&Utc))
}

fn truncate(value: &str, max_chars: usize) -> String {
    let mut truncated = value.to_owned();
    if truncated.len() > max_chars {
        truncated.truncate(max_chars);
    }
    truncated
}

async fn get_integration(
    state: &AppState,
    workspace_id: Uuid,
    source: Source,
) -> AppResult<Option<IntegrationResponse>> {
    sqlx::query_as::<_, IntegrationResponse>(integration_select_sql(
        r#"
        WHERE integrations.workspace_id = $1
          AND integrations.source = $2
          AND integrations.deleted_at IS NULL
        "#,
    ))
    .bind(workspace_id)
    .bind(source)
    .fetch_optional(&state.db)
    .await
    .map_err(AppError::Database)
}

fn integration_select_sql(where_clause: &str) -> String {
    format!(
        r#"
        SELECT integrations.source,
            health.last_event_at,
            COALESCE(health.events_24h, 0) AS events_24h,
            COALESCE(health.errors_24h, 0) AS errors_24h,
            COALESCE(health.status, 'active'::integration_status) AS status,
            (integrations.webhook_secret_enc IS NOT NULL OR integrations.webhook_secret_hash IS NOT NULL) AS has_webhook_secret,
            (integrations.api_token_enc IS NOT NULL) AS has_api_credential,
            COALESCE(integrations.api_sync_enabled, false) AS api_sync_enabled,
            COALESCE(integrations.sync_config, '{{}}'::jsonb) AS sync_config,
            integrations.last_sync_at
        FROM integrations
        LEFT JOIN integration_health AS health
          ON health.workspace_id = integrations.workspace_id
         AND health.source = integrations.source
        {where_clause}
        "#
    )
}

async fn dlq_values(state: &AppState, workspace_id: Uuid, limit: i64) -> AppResult<Vec<String>> {
    let mut redis = state
        .redis
        .get()
        .await
        .map_err(|error| AppError::Internal(anyhow!(error)))?;
    redis::cmd("LRANGE")
        .arg(dlq_list_key(workspace_id))
        .arg(0)
        .arg(limit.saturating_sub(1))
        .query_async::<Vec<String>>(&mut *redis)
        .await
        .map_err(|error| AppError::Internal(anyhow!(error)))
}

async fn find_dlq_value(
    state: &AppState,
    workspace_id: Uuid,
    job_id: Uuid,
) -> AppResult<Option<String>> {
    let mut redis = state
        .redis
        .get()
        .await
        .map_err(|error| AppError::Internal(anyhow!(error)))?;
    redis::cmd("GET")
        .arg(dlq_entry_key(workspace_id, job_id))
        .query_async::<Option<String>>(&mut *redis)
        .await
        .map_err(|error| AppError::Internal(anyhow!(error)))
}

fn parse_dlq_response(raw: &str) -> Option<DlqEntryResponse> {
    let entry = serde_json::from_str::<StoredDlqEntry>(raw).ok()?;
    let job_id = entry.job_id.or(entry.event_id).or(entry.memory_id)?;
    let payload_summary = entry
        .payload
        .map(|payload| summarize_payload(&payload))
        .unwrap_or_default();

    Some(DlqEntryResponse {
        job_id,
        payload_summary,
        error: entry.error.unwrap_or_default(),
        retry_count: entry.retry_count.unwrap_or_default(),
        failed_at: entry.failed_at,
    })
}

fn summarize_payload(payload: &Value) -> String {
    let mut summary = payload.to_string();
    if summary.len() > MAX_PAYLOAD_SUMMARY_CHARS {
        summary.truncate(MAX_PAYLOAD_SUMMARY_CHARS);
    }
    summary
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DlqRetryTarget {
    RawEvent(Uuid),
    SlowProcessorJob(Uuid),
}

fn parse_dlq_retry_target(raw: &str) -> Option<DlqRetryTarget> {
    let entry = serde_json::from_str::<StoredDlqEntry>(raw).ok()?;
    dlq_retry_target(&entry)
}

fn dlq_retry_target(entry: &StoredDlqEntry) -> Option<DlqRetryTarget> {
    if let Some(event_id) = entry.event_id {
        return Some(DlqRetryTarget::RawEvent(event_id));
    }

    if let Some(memory_id) = entry
        .memory_id
        .or_else(|| memory_id_from_payload(entry.payload.as_ref()))
    {
        return Some(DlqRetryTarget::SlowProcessorJob(memory_id));
    }

    entry.job_id.map(DlqRetryTarget::RawEvent)
}

fn memory_id_from_payload(payload: Option<&Value>) -> Option<Uuid> {
    payload?
        .get("memory_id")?
        .as_str()
        .and_then(|value| Uuid::parse_str(value).ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dlq_retry_target_uses_raw_event_stream_when_event_id_is_present() {
        let event_id = Uuid::now_v7();
        let raw = serde_json::json!({
            "job_id": event_id,
            "event_id": event_id,
            "payload": {}
        })
        .to_string();

        assert_eq!(
            parse_dlq_retry_target(&raw),
            Some(DlqRetryTarget::RawEvent(event_id))
        );
    }

    #[test]
    fn dlq_retry_target_uses_slow_processor_stream_for_memory_jobs() {
        let memory_id = Uuid::now_v7();
        let raw = serde_json::json!({
            "job_id": memory_id,
            "memory_id": memory_id,
            "payload": { "memory_id": memory_id }
        })
        .to_string();

        assert_eq!(
            parse_dlq_retry_target(&raw),
            Some(DlqRetryTarget::SlowProcessorJob(memory_id))
        );
    }

    #[test]
    fn dlq_retry_target_treats_job_id_only_entries_as_legacy_raw_events() {
        let event_id = Uuid::now_v7();
        let raw = serde_json::json!({
            "job_id": event_id,
            "payload": { "action": "opened" }
        })
        .to_string();

        assert_eq!(
            parse_dlq_retry_target(&raw),
            Some(DlqRetryTarget::RawEvent(event_id))
        );
    }

    #[test]
    fn dlq_response_can_display_memory_job_ids() {
        let memory_id = Uuid::now_v7();
        let raw = serde_json::json!({
            "memory_id": memory_id,
            "payload": { "memory_id": memory_id },
            "error": "embedding failed",
            "retry_count": 3
        })
        .to_string();

        let Some(response) = parse_dlq_response(&raw) else {
            panic!("DLQ response should parse");
        };

        assert_eq!(response.job_id, memory_id);
        assert_eq!(response.error, "embedding failed");
        assert_eq!(response.retry_count, 3);
    }

    #[test]
    fn normalize_github_repo_accepts_owner_name() {
        let repo = normalize_github_repo(Some("Quazmoz/memoryops"));
        assert_eq!(repo.unwrap(), "Quazmoz/memoryops");
    }

    #[test]
    fn normalize_github_repo_rejects_invalid_values() {
        assert!(normalize_github_repo(None).is_err());
        assert!(normalize_github_repo(Some("owner")).is_err());
        assert!(normalize_github_repo(Some("owner/repo/extra")).is_err());
        assert!(normalize_github_repo(Some("owner /repo")).is_err());
    }
}
