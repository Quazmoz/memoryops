use axum::{extract::Path, extract::State, http::StatusCode, Extension, Json};
use chrono::{DateTime, Utc};
use common::{
    audit::spawn_audit_log,
    auth::AuthContext,
    error::AppResult,
    models::{AuditAction, IntegrationStatus, Source},
    AppError, AppState,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use uuid::Uuid;

use crate::security::encrypt_secret;

pub use super::integration_dlq::{delete_dlq, list_dlq, retry_dlq};
pub use super::integration_sync::start_connector_sync;
use super::require_workspace;

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

#[axum::debug_handler]
#[tracing::instrument(skip(state, request))]
pub async fn create_integration(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthContext>,
    Path(id): Path<Uuid>,
    Json(request): Json<CreateIntegrationRequest>,
) -> AppResult<Json<IntegrationResponse>> {
    require_workspace(&auth, id)?;
    let webhook_secret = trimmed_optional(&request.webhook_secret);
    let api_token = trimmed_optional(&request.api_token);

    if webhook_secret.is_none() && api_token.is_none() {
        return Err(AppError::Validation(
            "webhook_secret or api_token is required".to_owned(),
        ));
    }

    let webhook_secret_enc = encrypted_optional(&state, webhook_secret.as_deref())?;
    let api_token_enc = encrypted_optional(&state, api_token.as_deref())?;
    let api_sync_enabled = request
        .api_sync_enabled
        .unwrap_or_else(|| api_token_enc.is_some());
    let sync_config = request.sync_config.unwrap_or_else(|| json!({}));

    upsert_integration(
        &state,
        id,
        request.source,
        webhook_secret_enc.as_deref(),
        api_token_enc.as_deref(),
        api_sync_enabled,
        &sync_config,
    )
    .await?;
    ensure_integration_health(&state, id, request.source).await?;

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
    let sql = integration_select_sql(
        r#"
        WHERE integrations.workspace_id = $1
          AND integrations.deleted_at IS NULL
        ORDER BY integrations.source::text ASC
        "#,
    );
    let integrations = sqlx::query_as::<_, IntegrationResponse>(&sql)
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

async fn upsert_integration(
    state: &AppState,
    workspace_id: Uuid,
    source: Source,
    webhook_secret_enc: Option<&str>,
    api_token_enc: Option<&str>,
    api_sync_enabled: bool,
    sync_config: &Value,
) -> AppResult<()> {
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
    .bind(workspace_id)
    .bind(source)
    .bind(Option::<&str>::None)
    .bind(webhook_secret_enc)
    .bind(api_token_enc)
    .bind(api_sync_enabled)
    .bind(sync_config)
    .execute(&state.db)
    .await
    .map_err(AppError::Database)?;

    Ok(())
}

async fn ensure_integration_health(
    state: &AppState,
    workspace_id: Uuid,
    source: Source,
) -> AppResult<()> {
    sqlx::query(
        r#"
        INSERT INTO integration_health (workspace_id, source)
        VALUES ($1, $2)
        ON CONFLICT (workspace_id, source) DO NOTHING
        "#,
    )
    .bind(workspace_id)
    .bind(source)
    .execute(&state.db)
    .await
    .map_err(AppError::Database)?;

    Ok(())
}

async fn get_integration(
    state: &AppState,
    workspace_id: Uuid,
    source: Source,
) -> AppResult<Option<IntegrationResponse>> {
    let sql = integration_select_sql(
        r#"
        WHERE integrations.workspace_id = $1
          AND integrations.source = $2
          AND integrations.deleted_at IS NULL
        "#,
    );
    sqlx::query_as::<_, IntegrationResponse>(&sql)
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

fn trimmed_optional(value: &Option<String>) -> Option<String> {
    value
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn encrypted_optional(state: &AppState, value: Option<&str>) -> AppResult<Option<String>> {
    value
        .map(|value| encrypt_secret(state.app_secret_key.as_ref().as_str(), value))
        .transpose()
}
