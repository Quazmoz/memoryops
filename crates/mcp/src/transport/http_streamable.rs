use std::{
    convert::Infallible,
    net::SocketAddr,
    sync::Arc,
    time::{Duration, Instant},
};

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
const DEFAULT_SESSION_TTL_SECS: u64 = 30 * 60;
const DEFAULT_MAX_SESSIONS: usize = 1024;

#[derive(Clone)]
struct HttpTransportState {
    server: SharedServer,
    sessions: Arc<DashMap<Uuid, SessionRecord>>,
}

#[derive(Debug, Clone)]
struct SessionRecord {
    token: String,
    last_seen: Instant,
}

impl HttpTransportState {
    fn prune_expired_sessions(&self) {
        let now = Instant::now();
        let ttl = session_ttl();
        self.sessions
            .retain(|_, session| now.duration_since(session.last_seen) <= ttl);
    }

    fn insert_session(&self, session_id: Uuid, token: String) -> Result<(), &'static str> {
        self.prune_expired_sessions();
        if self.sessions.len() >= max_sessions() {
            return Err("too many active MCP sessions");
        }

        self.sessions.insert(
            session_id,
            SessionRecord {
                token,
                last_seen: Instant::now(),
            },
        );
        Ok(())
    }

    fn validate_session_token(&self, session_id: Uuid, token: &str) -> Result<(), &'static str> {
        let now = Instant::now();
        let ttl = session_ttl();
        let Some(mut session) = self.sessions.get_mut(&session_id) else {
            return Err("invalid session ID");
        };

        if now.duration_since(session.last_seen) > ttl {
            drop(session);
            self.sessions.remove(&session_id);
            return Err("invalid session ID");
        }

        if session.token != token {
            return Err("invalid session credentials");
        }

        session.last_seen = now;
        Ok(())
    }

    fn touch_session(&self, session_id: Uuid) -> bool {
        let now = Instant::now();
        let ttl = session_ttl();
        let Some(mut session) = self.sessions.get_mut(&session_id) else {
            return false;
        };

        if now.duration_since(session.last_seen) > ttl {
            drop(session);
            self.sessions.remove(&session_id);
            return false;
        }

        session.last_seen = now;
        true
    }
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
        .route(
            "/mcp",
            post(handle_mcp_post)
                .get(handle_mcp_get)
                .delete(handle_mcp_delete),
        )
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
    state.prune_expired_sessions();

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
        if let Err(message) = state.validate_session_token(session_id, &token) {
            return bad_request(message);
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
    if let Err(message) = state.insert_session(new_session_id, token.to_owned()) {
        return service_unavailable(message);
    }

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

    if !state.touch_session(session_id) {
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

fn with_session_header(
    mut response: axum::response::Response,
    session_id: Uuid,
) -> axum::response::Response {
    let value = match HeaderValue::from_str(&session_id.to_string()) {
        Ok(value) => value,
        Err(_) => return response,
    };
    response.headers_mut().insert(SESSION_HEADER, value);
    response
}

fn json_response(
    status: StatusCode,
    body: Value,
    session_id: Option<Uuid>,
) -> axum::response::Response {
    let mut response = (status, Json(body)).into_response();
    if let Some(session_id) = session_id {
        response = with_session_header(response, session_id);
    }
    response
}

fn bad_request(message: &str) -> axum::response::Response {
    (StatusCode::BAD_REQUEST, Json(json!({ "error": message }))).into_response()
}

fn service_unavailable(message: &str) -> axum::response::Response {
    (
        StatusCode::SERVICE_UNAVAILABLE,
        Json(json!({ "error": message })),
    )
        .into_response()
}

fn session_ttl() -> Duration {
    Duration::from_secs(
        std::env::var("MCP_SESSION_TTL_SECS")
            .ok()
            .and_then(|value| value.parse::<u64>().ok())
            .filter(|value| *value > 0)
            .unwrap_or(DEFAULT_SESSION_TTL_SECS),
    )
}

fn max_sessions() -> usize {
    std::env::var("MCP_MAX_SESSIONS")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(DEFAULT_MAX_SESSIONS)
}
