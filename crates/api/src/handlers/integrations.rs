use anyhow::anyhow;
use axum::{extract::Path, extract::Query, extract::State, http::StatusCode, Extension, Json};
use chrono::{DateTime, Utc};
use common::{
    audit::spawn_audit_log,
    auth::AuthContext,
    error::AppResult,
    models::{AuditAction, IntegrationStatus, Source},
    AppError, AppState,
};
use ingestion::STREAM_KEY;
use processor::{
    dlq::{dlq_key as dlq_entry_key, dlq_list_key},
    worker::PROCESSOR_JOBS_STREAM,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use uuid::Uuid;

use crate::security::encrypt_secret;

use super::require_workspace;

const MAX_PAYLOAD_SUMMARY_CHARS: usize = 240;
const DEFAULT_DLQ_LIMIT: i64 = 100;
const MAX_DLQ_LIMIT: i64 = 500;

#[derive(Debug, Deserialize)]
pub struct CreateIntegrationRequest {
    pub source: Source,
    #[serde(default)]
    pub webhook_secret: Option<String>,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct IntegrationResponse {
    pub source: Source,
    pub last_event_at: Option<DateTime<Utc>>,
    pub events_24h: i64,
    pub errors_24h: i64,
    pub status: IntegrationStatus,
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
    if webhook_secret.is_none() {
        return Err(AppError::Validation(
            "webhook_secret is required".to_owned(),
        ));
    }

    let secret_enc = match webhook_secret.as_deref() {
        Some(secret) => Some(encrypt_secret(
            state.app_secret_key.as_ref().as_str(),
            secret,
        )?),
        None => None,
    };
    sqlx::query(
        r#"
        INSERT INTO integrations (workspace_id, source, webhook_secret_hash, webhook_secret_enc, deleted_at)
        VALUES ($1, $2, $3, $4, NULL)
        ON CONFLICT (workspace_id, source) DO UPDATE
        SET webhook_secret_hash = EXCLUDED.webhook_secret_hash,
            webhook_secret_enc = EXCLUDED.webhook_secret_enc,
            deleted_at = NULL
        "#,
    )
    .bind(id)
    .bind(request.source)
    .bind(Option::<&str>::None)
    .bind(secret_enc.as_deref())
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
        Some(json!({ "source": request.source })),
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
        r#"
        SELECT integrations.source,
            health.last_event_at,
            COALESCE(health.events_24h, 0) AS events_24h,
            COALESCE(health.errors_24h, 0) AS errors_24h,
            COALESCE(health.status, 'active'::integration_status) AS status
        FROM integrations
        LEFT JOIN integration_health AS health
          ON health.workspace_id = integrations.workspace_id
         AND health.source = integrations.source
        WHERE integrations.workspace_id = $1
          AND integrations.deleted_at IS NULL
        ORDER BY integrations.source::text ASC
        "#,
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

async fn get_integration(
    state: &AppState,
    workspace_id: Uuid,
    source: Source,
) -> AppResult<Option<IntegrationResponse>> {
    sqlx::query_as::<_, IntegrationResponse>(
        r#"
        SELECT integrations.source,
            health.last_event_at,
            COALESCE(health.events_24h, 0) AS events_24h,
            COALESCE(health.errors_24h, 0) AS errors_24h,
            COALESCE(health.status, 'active'::integration_status) AS status
        FROM integrations
        LEFT JOIN integration_health AS health
          ON health.workspace_id = integrations.workspace_id
         AND health.source = integrations.source
        WHERE integrations.workspace_id = $1
          AND integrations.source = $2
          AND integrations.deleted_at IS NULL
        "#,
    )
    .bind(workspace_id)
    .bind(source)
    .fetch_optional(&state.db)
    .await
    .map_err(AppError::Database)
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
}
