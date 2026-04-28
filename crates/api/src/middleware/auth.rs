use axum::{
    body::Body,
    extract::State,
    http::{header::HeaderName, Method, Request},
    middleware::Next,
    response::Response,
};
use common::{
    auth::{spawn_last_used_update, validate_api_key},
    error::AppResult,
    AppError, AppState,
};

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
    let context = validate_api_key(&state.db, api_key).await?;
    spawn_last_used_update(state.db.clone(), context.key_id);
    request.extensions_mut().insert(context);

    Ok(next.run(request).await)
}

fn is_first_key_bootstrap_request(request: &Request<Body>) -> bool {
    request.method() == Method::POST
        && request.uri().path().starts_with("/v1/workspaces/")
        && request.uri().path().ends_with("/keys")
}
