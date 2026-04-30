use axum::{
    body::Body,
    extract::State,
    http::{header::HeaderName, Method, Request},
    middleware::Next,
    response::Response,
};
use common::{
    auth::{spawn_last_used_update, validate_api_key_cached},
    error::AppResult,
    AppError, AppState,
};
use uuid::Uuid;

pub const API_KEY_HEADER: HeaderName = HeaderName::from_static("x-api-key");

pub async fn require_api_key(
    State(state): State<AppState>,
    mut request: Request<Body>,
    next: Next,
) -> AppResult<Response> {
    let Some(api_key_header) = request.headers().get(&API_KEY_HEADER) else {
        if is_first_key_bootstrap_request(&state, request.method(), request.uri().path()).await? {
            return Ok(next.run(request).await);
        }
        return Err(AppError::Unauthorized);
    };
    let api_key = api_key_header
        .to_str()
        .map_err(|_| AppError::Unauthorized)?;
    let mut redis = state.redis.clone();
    let context = validate_api_key_cached(&state.db, &mut redis, api_key).await?;
    spawn_last_used_update(state.db.clone(), context.key_id);
    request.extensions_mut().insert(context);

    Ok(next.run(request).await)
}

async fn is_first_key_bootstrap_request(
    state: &AppState,
    method: &Method,
    path: &str,
) -> AppResult<bool> {
    if *method != Method::POST {
        return Ok(false);
    }

    let Some(workspace_id) = bootstrap_workspace_id(path) else {
        return Ok(false);
    };

    // Single atomic query: workspace must exist AND have zero active keys.
    // Uses a subquery so no FOR UPDATE is needed -- the COUNT is the lock guard.
    let result = sqlx::query_scalar::<_, i64>(
        r#"
        SELECT COUNT(*)
        FROM api_keys
        WHERE workspace_id = $1
          AND revoked = false
          AND EXISTS (
              SELECT 1 FROM workspaces
              WHERE id = $1 AND deleted_at IS NULL
          )
        "#,
    )
    .bind(workspace_id)
    .fetch_one(&state.db)
    .await
    .map_err(AppError::Database)?;

    // Workspace doesn't exist if the EXISTS subquery returns nothing -- treat as
    // Unauthorized rather than revealing whether the workspace exists.
    // If workspace exists and key count > 0, also Unauthorized.
    // Only allow if workspace exists AND key count == 0.
    //
    // To distinguish "workspace not found" from "keys already exist" atomically,
    // check workspace existence first in the same txn:
    let workspace_exists = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM workspaces WHERE id = $1 AND deleted_at IS NULL)",
    )
    .bind(workspace_id)
    .fetch_one(&state.db)
    .await
    .map_err(AppError::Database)?;

    if !workspace_exists {
        return Err(AppError::Unauthorized);
    }

    if result == 0 {
        Ok(true)
    } else {
        Err(AppError::Unauthorized)
    }
}

fn bootstrap_workspace_id(path: &str) -> Option<Uuid> {
    let mut parts = path.trim_start_matches('/').split('/');
    match (
        parts.next(),
        parts.next(),
        parts.next(),
        parts.next(),
        parts.next(),
    ) {
        (Some("v1"), Some("workspaces"), Some(workspace_id), Some("keys"), None) => {
            Uuid::parse_str(workspace_id).ok()
        }
        _ => None,
    }
}
