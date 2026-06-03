use axum::{
    extract::{Path, Query, State},
    Extension, Json,
};
use chrono::{DateTime, Utc};
use common::{
    audit::spawn_audit_log,
    auth::{invalidate_api_key_cache, AuthContext},
    error::AppResult,
    models::AuditAction,
    AppError, AppState,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::PgPool;
use uuid::Uuid;

use crate::security::{generate_api_key, hash_secret};

use super::require_workspace;

#[derive(Debug, Deserialize)]
pub struct CreateKeyRequest {
    pub name: String,
}

#[derive(Debug, Serialize)]
pub struct CreateKeyResponse {
    pub id: Uuid,
    pub name: String,
    pub prefix: String,
    pub key: String,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct ApiKeySummary {
    pub id: Uuid,
    pub name: String,
    pub prefix: String,
    pub created_at: DateTime<Utc>,
    pub last_used_at: Option<DateTime<Utc>>,
    pub revoked: bool,
}

pub type KeyRecord = ApiKeySummary;

#[derive(Debug, Deserialize)]
pub struct ListKeysQuery {
    #[serde(default)]
    pub include_revoked: bool,
}

/// Inserts a new API key record for a workspace and returns the plaintext key once.
pub async fn insert_key(
    db: &PgPool,
    workspace_id: Uuid,
    name: &str,
) -> AppResult<(String, KeyRecord)> {
    let mut tx = db.begin().await.map_err(AppError::Database)?;
    let inserted = insert_key_record(&mut tx, workspace_id, name).await?;
    tx.commit().await.map_err(AppError::Database)?;
    Ok(inserted)
}

async fn insert_key_record(
    conn: &mut sqlx::PgConnection,
    workspace_id: Uuid,
    name: &str,
) -> AppResult<(String, KeyRecord)> {
    let key_id = Uuid::now_v7();
    let (plaintext, prefix) = generate_api_key(workspace_id)?;
    let key_hash = hash_secret(&plaintext)?;
    let created = sqlx::query_as::<_, ApiKeySummary>(
        r#"
        INSERT INTO api_keys (id, workspace_id, name, key_hash, prefix, prefix_version)
        VALUES ($1, $2, $3, $4, $5, 3)
        RETURNING id, name, prefix, created_at, last_used_at, revoked
        "#,
    )
    .bind(key_id)
    .bind(workspace_id)
    .bind(name)
    .bind(key_hash)
    .bind(&prefix)
    .fetch_one(conn)
    .await
    .map_err(AppError::Database)?;

    Ok((plaintext, created))
}

#[axum::debug_handler]
pub async fn create_key(
    State(state): State<AppState>,
    auth: Option<Extension<AuthContext>>,
    Path(id): Path<Uuid>,
    Json(request): Json<CreateKeyRequest>,
) -> AppResult<Json<CreateKeyResponse>> {
    if request.name.trim().is_empty() {
        return Err(AppError::Validation("key name is required".to_owned()));
    }

    let name = request.name.trim().to_owned();
    let (actor, plaintext, created) = match auth.as_ref() {
        Some(auth) => {
            require_workspace(&auth.0, id)?;
            let (plaintext, created) = insert_key(&state.db, id, &name).await?;
            (auth.0.actor(), plaintext, created)
        }
        None => {
            let mut tx = ensure_first_key_bootstrap(&state, id).await?;
            let (plaintext, created) = insert_key_record(&mut tx, id, &name).await?;
            tx.commit().await.map_err(AppError::Database)?;
            ("bootstrap".to_owned(), plaintext, created)
        }
    };

    spawn_audit_log(
        state.db.clone(),
        id,
        actor,
        AuditAction::KeyCreated,
        created.id,
        "api_key",
        Some(json!({ "name": created.name, "prefix": created.prefix })),
    );

    Ok(Json(CreateKeyResponse {
        id: created.id,
        name,
        prefix: created.prefix,
        key: plaintext,
    }))
}

/// Validates that no active API keys exist for the workspace before allowing
/// unauthenticated first-key bootstrap. Uses a serialized transaction with
/// FOR UPDATE to prevent concurrent duplicate bootstrap key creation.
/// Returns the open transaction so the bootstrap insert commits under the same lock.
async fn ensure_first_key_bootstrap(
    state: &AppState,
    workspace_id: Uuid,
) -> AppResult<sqlx::Transaction<'static, sqlx::Postgres>> {
    let mut tx = state.db.begin().await.map_err(AppError::Database)?;

    let workspace_exists = sqlx::query_scalar::<_, bool>(
        r#"
        SELECT EXISTS(
            SELECT 1 FROM workspaces
            WHERE id = $1 AND deleted_at IS NULL
            FOR UPDATE
        )
        "#,
    )
    .bind(workspace_id)
    .fetch_one(&mut *tx)
    .await
    .map_err(AppError::Database)?;

    if !workspace_exists {
        tx.rollback().await.map_err(AppError::Database)?;
        return Err(AppError::NotFound {
            resource: format!("workspace:{workspace_id}"),
        });
    }

    let active_key_count = sqlx::query_scalar::<_, i64>(
        r#"
        SELECT COUNT(*) FROM api_keys
        WHERE workspace_id = $1 AND revoked = false
        "#,
    )
    .bind(workspace_id)
    .fetch_one(&mut *tx)
    .await
    .map_err(AppError::Database)?;

    if active_key_count == 0 {
        Ok(tx)
    } else {
        tx.rollback().await.map_err(AppError::Database)?;
        Err(AppError::Unauthorized)
    }
}

#[axum::debug_handler]
pub async fn list_keys(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthContext>,
    Path(id): Path<Uuid>,
    Query(query): Query<ListKeysQuery>,
) -> AppResult<Json<Vec<ApiKeySummary>>> {
    require_workspace(&auth, id)?;
    let keys = sqlx::query_as::<_, ApiKeySummary>(
        r#"
        SELECT id, name, prefix, created_at, last_used_at, revoked
        FROM api_keys
        WHERE workspace_id = $1
          AND ($2::boolean OR revoked = false)
        ORDER BY created_at DESC
        "#,
    )
    .bind(id)
    .bind(query.include_revoked)
    .fetch_all(&state.db)
    .await
    .map_err(AppError::Database)?;

    Ok(Json(keys))
}

#[axum::debug_handler]
pub async fn revoke_key(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthContext>,
    Path((id, key_id)): Path<(Uuid, Uuid)>,
) -> AppResult<Json<ApiKeySummary>> {
    require_workspace(&auth, id)?;
    let revoked = sqlx::query_as::<_, ApiKeySummary>(
        r#"
        UPDATE api_keys
        SET revoked = true, revoked_at = now()
        WHERE workspace_id = $1 AND id = $2 AND revoked = false
        RETURNING id, name, prefix, created_at, last_used_at, revoked
        "#,
    )
    .bind(id)
    .bind(key_id)
    .fetch_optional(&state.db)
    .await
    .map_err(AppError::Database)?
    .ok_or_else(|| AppError::NotFound {
        resource: format!("api_key:{key_id}"),
    })?;

    let mut redis = state
        .redis
        .get()
        .await
        .map_err(|_| AppError::Internal(anyhow::anyhow!("redis pool error")))?;
    if let Err(error) = invalidate_api_key_cache(&mut *redis, key_id).await {
        tracing::error!(
            error = ?error,
            key_id = %key_id,
            "failed to invalidate revoked API key cache; revoked key may remain authorized for up to 60 seconds"
        );
    }

    spawn_audit_log(
        state.db.clone(),
        id,
        auth.actor(),
        AuditAction::KeyRevoked,
        key_id,
        "api_key",
        Some(json!({ "prefix": revoked.prefix })),
    );

    Ok(Json(revoked))
}
