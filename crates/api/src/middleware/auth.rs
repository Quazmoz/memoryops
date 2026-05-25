use axum::{
    body::Body,
    extract::State,
    http::{header::HeaderName, Method, Request},
    middleware::Next,
    response::Response,
};
use common::{
    error::AppResult,
    services::AuthService,
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
    let context = AuthService::from_state(&state)
        .authenticate_api_key(api_key)
        .await?;
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
        return Err(AppError::Unauthorized);
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

    tx.commit().await.map_err(AppError::Database)?;

    if active_key_count == 0 {
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
