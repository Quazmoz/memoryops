//! Captures per-request context (request id, client IP, user agent, method,
//! path) into a [`RequestContext`] extension so handlers can attach it to audit
//! events. The client IP honours trusted-proxy rules; sensitive headers
//! (authorization, x-api-key, cookies) are never recorded.

use axum::{body::Body, extract::State, http::Request, middleware::Next, response::Response};
use common::{audit::RequestContext, AppState};

use super::{rate_limit::resolve_client_ip, request_id::REQUEST_ID_HEADER};

const CORRELATION_ID_HEADER: &str = "x-correlation-id";
const USER_AGENT_HEADER: &str = "user-agent";
/// Bound captured free-text header values so a hostile client cannot bloat audit rows.
const MAX_HEADER_VALUE_LEN: usize = 512;

pub async fn request_context(
    State(state): State<AppState>,
    mut request: Request<Body>,
    next: Next,
) -> Response {
    let request_id = header_string(&request, REQUEST_ID_HEADER.as_str());
    let correlation_id = header_string(&request, CORRELATION_ID_HEADER);
    let user_agent = header_string(&request, USER_AGENT_HEADER);
    let method = Some(request.method().as_str().to_owned());
    let route = Some(request.uri().path().to_owned());
    let source_ip =
        resolve_client_ip(&request, &state.trusted_proxy_cidrs).map(|ip| ip.to_string());

    request.extensions_mut().insert(RequestContext {
        request_id,
        correlation_id,
        source_ip,
        user_agent,
        method,
        route,
    });

    next.run(request).await
}

fn header_string(request: &Request<Body>, name: &str) -> Option<String> {
    request
        .headers()
        .get(name)
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.chars().take(MAX_HEADER_VALUE_LEN).collect())
}
