use axum::{
    body::Body,
    extract::State,
    http::{header::HeaderName, Method, Request},
    middleware::Next,
    response::Response,
};
use common::{auth::AuthContext, error::AppResult, models::ApiKey, AppError, AppState};

use crate::security::{api_key_prefix, verify_secret};

pub const API_KEY_HEADER: HeaderName = HeaderName::from_static("x-api-key");

pub async fn require_api_key(
    State(state): State<AppState>,
    mut request: Request<Body>,
    next: Next,
) -> AppResult<Response> {
    let Some(api_key_header) = request.headers().get(&API_KEY_HEADER) else {
        if is_first_key_bootstrap_request(&request) {
            return Ok(next.run(request).await);
        }
        return Err(AppError::Unauthorized);
    };
    let api_key = api_key_header
        .to_str()
        .map_err(|_| AppError::Unauthorized)?;
    let prefix = api_key_prefix(api_key).ok_or(AppError::Unauthorized)?;
    let candidates = find_candidate_keys(&state, &prefix).await?;

    for candidate in candidates {
        if verify_secret(api_key, &candidate.key_hash) {
            if candidate.revoked {
                return Err(AppError::Unauthorized);
            }

            let context = AuthContext {
                workspace_id: candidate.workspace_id,
                key_id: candidate.id,
                key_prefix: candidate.prefix.clone(),
            };
            request.extensions_mut().insert(context);
            spawn_last_used_update(state.clone(), candidate.id);
            return Ok(next.run(request).await);
        }
    }

    Err(AppError::Unauthorized)
}

fn is_first_key_bootstrap_request(request: &Request<Body>) -> bool {
    request.method() == Method::POST
        && request.uri().path().starts_with("/v1/workspaces/")
        && request.uri().path().ends_with("/keys")
}

async fn find_candidate_keys(state: &AppState, prefix: &str) -> AppResult<Vec<ApiKey>> {
    sqlx::query_as::<_, ApiKey>(
        r#"
        SELECT id, workspace_id, name, key_hash, prefix, created_at, last_used_at, revoked, revoked_at
        FROM api_keys
        WHERE prefix = $1
        "#,
    )
    .bind(prefix)
    .fetch_all(&state.db)
    .await
    .map_err(AppError::Database)
}

fn spawn_last_used_update(state: AppState, key_id: uuid::Uuid) {
    let db = state.db.clone();
    tokio::spawn(async move {
        if let Err(error) = sqlx::query("UPDATE api_keys SET last_used_at = now() WHERE id = $1")
            .bind(key_id)
            .execute(&db)
            .await
        {
            tracing::warn!(error = ?error, key_id = %key_id, "failed to update API key last_used_at");
        }
    });
}
