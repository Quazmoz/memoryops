use std::{convert::Infallible, net::SocketAddr, sync::Arc};

use axum::{
    extract::State,
    http::{HeaderMap, HeaderValue, StatusCode},
    response::{
        sse::{Event, KeepAlive, Sse},
        IntoResponse,
    },
    routing::{get, post},
    Json, Router,
};
use dashmap::DashMap;
use futures_util::stream;
use serde_json::{json, Value};
use tower_http::cors::CorsLayer;
use uuid::Uuid;

use crate::{auth, server::SharedServer};

const DEFAULT_PORT: u16 = 3003;
const SESSION_HEADER: &str = "Mcp-Session-Id";

#[derive(Clone)]
struct HttpTransportState {
    server: SharedServer,
    sessions: Arc<DashMap<Uuid, String>>,
}

/// Runs the MCP HTTP Streamable transport server.
pub async fn run(server: SharedServer) -> anyhow::Result<()> {
    let port = port_from_env();
    let address = SocketAddr::from(([0, 0, 0, 0], port));
    let listener = tokio::net::TcpListener::bind(address).await?;

    tracing::info!(%address, "starting MemoryOps MCP HTTP streamable transport");
    axum::serve(listener, router(server)).await?;

    Ok(())
}

/// Builds the MCP HTTP Streamable transport router.
pub fn router(server: SharedServer) -> Router {
    let state = HttpTransportState {
        server,
        sessions: Arc::new(DashMap::new()),
    };

    Router::new()
        .route("/mcp", post(handle_mcp_post).get(handle_mcp_get).delete(handle_mcp_delete))
        .route("/health", get(health))
        .layer(CorsLayer::permissive())
        .with_state(state)
}

/// Returns the MCP HTTP transport port from MCP_PORT or default 3003.
pub fn port_from_env() -> u16 {
    std::env::var("MCP_PORT")
        .ok()
        .and_then(|port| port.parse::<u16>().ok())
        .unwrap_or(DEFAULT_PORT)
}

async fn handle_mcp_post(
    State(state): State<HttpTransportState>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> axum::response::Response {
    let token = match auth::header_bearer_token(&headers) {
        Some(token) => token,
        None => {
            return bad_request("Authorization: Bearer <api_key> header is required");
        }
    };

    let session_id = match session_id_from_headers(&headers) {
        Ok(session_id) => session_id,
        Err(message) => return bad_request(message),
    };

    if let Some(session_id) = session_id {
        let Some(stored_token) = state.sessions.get(&session_id) else {
            return bad_request("invalid session ID");
        };
        if stored_token.value() != &token {
            return bad_request("invalid session credentials");
        }

        if body.get("id").is_none() {
            let _ = state
                .server
                .handle_http_message(body, Some(token.to_owned()))
                .await;
            return StatusCode::ACCEPTED.into_response();
        }

        let response = state
            .server
            .handle_http_message(body, Some(token.to_owned()))
            .await;
        return json_response(StatusCode::OK, response, None);
    }

    if !is_initialize_request(&body) {
        return bad_request("Mcp-Session-Id header is required after initialize");
    }

    let new_session_id = Uuid::now_v7();
    state.sessions.insert(new_session_id, token.to_owned());

    if body.get("id").is_none() {
        let _ = state
            .server
            .handle_http_message(body, Some(token.to_owned()))
            .await;
        return with_session_header(StatusCode::ACCEPTED.into_response(), new_session_id);
    }

    let response = state
        .server
        .handle_http_message(body, Some(token.to_owned()))
        .await;
    let response = json_response(StatusCode::OK, response, None);
    with_session_header(response, new_session_id)
}

async fn handle_mcp_get(
    State(state): State<HttpTransportState>,
    headers: HeaderMap,
) -> axum::response::Response {
    let session_id = match required_session_id(&headers) {
        Ok(session_id) => session_id,
        Err(message) => return bad_request(message),
    };

    if !state.sessions.contains_key(&session_id) {
        return bad_request("invalid session ID");
    }

    let stream = stream::pending::<Result<Event, Infallible>>();
    Sse::new(stream)
        .keep_alive(KeepAlive::default())
        .into_response()
}

async fn handle_mcp_delete(
    State(state): State<HttpTransportState>,
    headers: HeaderMap,
) -> axum::response::Response {
    let session_id = match required_session_id(&headers) {
        Ok(session_id) => session_id,
        Err(message) => return bad_request(message),
    };

    if state.sessions.remove(&session_id).is_none() {
        return bad_request("invalid session ID");
    }

    StatusCode::OK.into_response()
}

async fn health() -> (StatusCode, Json<Value>) {
    (StatusCode::OK, Json(json!({ "status": "ok" })))
}

fn session_id_from_headers(headers: &HeaderMap) -> Result<Option<Uuid>, &'static str> {
    let Some(raw) = headers.get(SESSION_HEADER) else {
        return Ok(None);
    };

    let value = raw.to_str().map_err(|_| "invalid Mcp-Session-Id header")?;
    let parsed = Uuid::parse_str(value.trim()).map_err(|_| "invalid Mcp-Session-Id header")?;

    Ok(Some(parsed))
}

fn required_session_id(headers: &HeaderMap) -> Result<Uuid, &'static str> {
    session_id_from_headers(headers)?.ok_or("Mcp-Session-Id header is required")
}

fn is_initialize_request(body: &Value) -> bool {
    body.get("method")
        .and_then(Value::as_str)
        .is_some_and(|method| method == "initialize")
}

fn with_session_header(mut response: axum::response::Response, session_id: Uuid) -> axum::response::Response {
    let value = match HeaderValue::from_str(&session_id.to_string()) {
        Ok(value) => value,
        Err(_) => return response,
    };
    response.headers_mut().insert(SESSION_HEADER, value);
    response
}

fn json_response(status: StatusCode, body: Value, session_id: Option<Uuid>) -> axum::response::Response {
    let mut response = (status, Json(body)).into_response();
    if let Some(session_id) = session_id {
        response = with_session_header(response, session_id);
    }
    response
}

fn bad_request(message: &str) -> axum::response::Response {
    (
        StatusCode::BAD_REQUEST,
        Json(json!({ "error": message })),
    )
        .into_response()
}
