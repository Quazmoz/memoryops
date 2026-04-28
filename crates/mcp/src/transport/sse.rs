use std::{convert::Infallible, net::SocketAddr};

use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{
        sse::{Event, KeepAlive, Sse},
        IntoResponse,
    },
    routing::{get, post},
    Json, Router,
};
use serde_json::{json, Value};
use tower_http::cors::CorsLayer;

use crate::{auth, server::SharedServer};

const DEFAULT_PORT: u16 = 3003;

pub async fn run(server: SharedServer) -> anyhow::Result<()> {
    let port = port_from_env();
    let address = SocketAddr::from(([0, 0, 0, 0], port));
    let listener = tokio::net::TcpListener::bind(address).await?;

    tracing::info!(%address, "starting MemoryOps MCP SSE transport");
    axum::serve(listener, router(server)).await?;

    Ok(())
}

pub fn router(server: SharedServer) -> Router {
    Router::new()
        .route("/mcp", post(handle_mcp))
        .route("/mcp/sse", get(handle_sse))
        .route("/health", get(health))
        .layer(CorsLayer::permissive())
        .with_state(server)
}

pub fn port_from_env() -> u16 {
    std::env::var("MCP_PORT")
        .ok()
        .and_then(|port| port.parse::<u16>().ok())
        .unwrap_or(DEFAULT_PORT)
}

async fn handle_mcp(
    State(server): State<SharedServer>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Json<Value> {
    let token = auth::header_bearer_token(&headers);
    Json(server.handle_http_message(body, token).await)
}

async fn handle_sse() -> impl IntoResponse {
    let stream = async_stream::stream! {
        yield Ok::<Event, Infallible>(
            Event::default()
                .event("ready")
                .data(json!({ "status": "ready" }).to_string()),
        );
    };

    Sse::new(stream).keep_alive(KeepAlive::default())
}

async fn health() -> (StatusCode, Json<Value>) {
    (StatusCode::OK, Json(json!({ "status": "ok" })))
}
