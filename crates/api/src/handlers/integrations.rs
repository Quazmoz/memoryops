use anyhow::anyhow;
use axum::{extract::Path, extract::State, http::StatusCode, Extension, Json};
use chrono::{DateTime, Utc};
use common::{
    audit::spawn_audit_log,
    auth::AuthContext,
    error::AppResult,
    models::{AuditAction, IntegrationStatus, Source},
    AppError, AppState,
};
use ingestion::STREAM_KEY;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use uuid::Uuid;

use crate::security::encrypt_secret;

use super::require_workspace;

const DLQ_LIST_PREFIX: &str = "dlq:";
const MAX_PAYLOAD_SUMMARY_CHARS: usize = 240;

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
struct StoredDlqEntry {
    pub job_id: Option<Uuid>,
    pub event_id: Option<Uuid>,
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
    if webhook_secret.is_none() && request.source != Source::Linear {
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
) -> AppResult<Json<Vec<DlqEntryResponse>>> {
    require_workspace(&auth, id)?;
    let values = dlq_values(&state, id).await?;
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
    let mut redis = state
        .redis
        .get()
        .await
        .map_err(|error| AppError::Internal(anyhow!(error)))?;
    let key = dlq_key(id);
    redis::pipe()
        .cmd("LREM")
        .arg(&key)
        .arg(1)
        .arg(&raw)
        .cmd("XADD")
        .arg(STREAM_KEY)
        .arg("*")
        .arg("event_id")
        .arg(job_id.to_string())
        .arg("workspace_id")
        .arg(id.to_string())
        .query_async::<(i64, String)>(&mut *redis)
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
    redis::cmd("LREM")
        .arg(dlq_key(id))
        .arg(1)
        .arg(raw)
        .query_async::<i64>(&mut *redis)
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

async fn dlq_values(state: &AppState, workspace_id: Uuid) -> AppResult<Vec<String>> {
    let mut redis = state
        .redis
        .get()
        .await
        .map_err(|error| AppError::Internal(anyhow!(error)))?;
    redis::cmd("LRANGE")
        .arg(dlq_key(workspace_id))
        .arg(0)
        .arg(-1)
        .query_async::<Vec<String>>(&mut *redis)
        .await
        .map_err(|error| AppError::Internal(anyhow!(error)))
}

async fn find_dlq_value(
    state: &AppState,
    workspace_id: Uuid,
    job_id: Uuid,
) -> AppResult<Option<String>> {
    let values = dlq_values(state, workspace_id).await?;
    Ok(values
        .into_iter()
        .find(|raw| dlq_job_id(raw) == Some(job_id)))
}

fn parse_dlq_response(raw: &str) -> Option<DlqEntryResponse> {
    let entry = serde_json::from_str::<StoredDlqEntry>(raw).ok()?;
    let job_id = entry.job_id.or(entry.event_id)?;
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

fn dlq_job_id(raw: &str) -> Option<Uuid> {
    let entry = serde_json::from_str::<StoredDlqEntry>(raw).ok()?;
    entry.job_id.or(entry.event_id)
}

fn summarize_payload(payload: &Value) -> String {
    let mut summary = payload.to_string();
    if summary.len() > MAX_PAYLOAD_SUMMARY_CHARS {
        summary.truncate(MAX_PAYLOAD_SUMMARY_CHARS);
    }
    summary
}

fn dlq_key(workspace_id: Uuid) -> String {
    format!("{DLQ_LIST_PREFIX}{workspace_id}")
}
