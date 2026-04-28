use axum::{
    body::Bytes,
    extract::State,
    http::{HeaderMap, StatusCode},
    Json,
};
use common::{models::Source, telemetry::INGEST_EVENTS, AppError, AppState};
use serde::Serialize;
use uuid::Uuid;

use crate::{
    github::{parser::parse_github_event, signature::verify_signature},
    queue::publish_raw_event,
    store::{
        find_raw_event_id_by_idempotency_key, insert_raw_event, workspace_exists, NewRawEvent,
    },
};

const SIGNATURE_HEADER: &str = "x-hub-signature-256";
const EVENT_HEADER: &str = "x-github-event";
const DELIVERY_HEADER: &str = "x-github-delivery";
const WORKSPACE_HEADER: &str = "x-workspace-id";

#[derive(Debug, Serialize)]
pub struct IngestResponse {
    status: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    event_id: Option<Uuid>,
}

#[axum::debug_handler]
pub async fn handle_github_webhook(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<(StatusCode, Json<IngestResponse>), AppError> {
    let signature = signature_header(&headers)?;
    let event_header = required_header(&headers, EVENT_HEADER)?;
    let delivery_id = required_header(&headers, DELIVERY_HEADER)?;
    let workspace_id = workspace_id_header(&headers)?;

    verify_signature(signature, &body, &state.github_webhook_secret)?;

    if !workspace_exists(&state.db, workspace_id).await? {
        return Err(AppError::NotFound {
            resource: format!("workspace {workspace_id}"),
        });
    }

    let payload = serde_json::from_slice(&body)
        .map_err(|error| AppError::Validation(format!("invalid JSON payload: {error}")))?;
    let parsed = parse_github_event(event_header, &payload)?;
    let idempotency_key = format!("github:{delivery_id}");

    if find_raw_event_id_by_idempotency_key(&state.db, &idempotency_key)
        .await?
        .is_some()
    {
        return Ok((
            StatusCode::OK,
            Json(IngestResponse {
                status: "duplicate",
                event_id: None,
            }),
        ));
    }

    let event = insert_raw_event(
        &state.db,
        &NewRawEvent {
            workspace_id,
            source: Source::GitHub,
            event_type: parsed.event_type,
            actor: parsed.actor,
            payload: parsed.payload,
            idempotency_key,
            occurred_at: parsed.occurred_at,
        },
    )
    .await?;
    INGEST_EVENTS.add(1, &[]);

    let mut redis = state.redis.clone();
    publish_raw_event(&mut redis, &event).await?;

    Ok((
        StatusCode::ACCEPTED,
        Json(IngestResponse {
            status: "accepted",
            event_id: Some(event.id),
        }),
    ))
}

fn signature_header(headers: &HeaderMap) -> Result<&str, AppError> {
    headers
        .get(SIGNATURE_HEADER)
        .ok_or(AppError::Unauthorized)?
        .to_str()
        .map_err(|_| AppError::Unauthorized)
}

fn required_header<'a>(headers: &'a HeaderMap, name: &str) -> Result<&'a str, AppError> {
    headers
        .get(name)
        .ok_or_else(|| AppError::Validation(format!("missing {name} header")))?
        .to_str()
        .map_err(|_| AppError::Validation(format!("invalid {name} header")))
}

fn workspace_id_header(headers: &HeaderMap) -> Result<Uuid, AppError> {
    let raw = required_header(headers, WORKSPACE_HEADER)?;
    Uuid::parse_str(raw)
        .map_err(|_| AppError::Validation("invalid x-workspace-id header".to_owned()))
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use async_trait::async_trait;
    use axum::{
        body::Body,
        http::{Method, Request},
    };
    use chrono::Utc;
    use common::{
        config::AppConfig,
        providers::{EmbeddingProvider, LlmProvider},
        ProviderError,
    };
    use hmac::{Hmac, Mac};
    use qdrant_client::Qdrant;
    use redis::aio::ConnectionManager;
    use serde_json::json;
    use sha2::Sha256;
    use sqlx::PgPool;
    use tower::ServiceExt;

    use crate::router::ingestion_router;

    use super::*;

    type HmacSha256 = Hmac<Sha256>;

    struct TestEmbeddingProvider;

    #[async_trait]
    impl EmbeddingProvider for TestEmbeddingProvider {
        async fn embed(&self, _text: &str) -> Result<Vec<f32>, ProviderError> {
            Err(ProviderError::NotConfigured)
        }

        async fn embed_batch(&self, _texts: &[&str]) -> Result<Vec<Vec<f32>>, ProviderError> {
            Err(ProviderError::NotConfigured)
        }

        fn dimensions(&self) -> usize {
            0
        }

        fn model_name(&self) -> &str {
            "not-configured"
        }
    }

    struct TestLlmProvider;

    #[async_trait]
    impl LlmProvider for TestLlmProvider {
        async fn complete(&self, _prompt: &str) -> Result<String, ProviderError> {
            Err(ProviderError::NotConfigured)
        }

        async fn summarize(
            &self,
            _text: &str,
            _max_tokens: usize,
        ) -> Result<String, ProviderError> {
            Err(ProviderError::NotConfigured)
        }
    }

    async fn test_state(pool: PgPool, secret: &str) -> AppState {
        let redis_url =
            std::env::var("REDIS_URL").unwrap_or_else(|_| "redis://localhost:16379".to_owned());
        let redis_client = match redis::Client::open(redis_url) {
            Ok(client) => client,
            Err(error) => panic!("test Redis URL should be valid: {error}"),
        };
        let redis = match ConnectionManager::new(redis_client).await {
            Ok(connection) => connection,
            Err(error) => panic!("test Redis should be reachable: {error}"),
        };
        let qdrant_url =
            std::env::var("QDRANT_URL").unwrap_or_else(|_| "http://localhost:16333".to_owned());
        let qdrant = match Qdrant::from_url(&qdrant_url).build() {
            Ok(client) => client,
            Err(error) => panic!("test Qdrant URL should be valid: {error}"),
        };
        let config = match AppConfig::from_toml_str(include_str!("../../../../config.toml")) {
            Ok(config) => config,
            Err(error) => panic!("checked-in config should parse: {error}"),
        };

        AppState {
            db: pool,
            redis,
            qdrant,
            embedding_provider: Arc::new(TestEmbeddingProvider),
            llm_provider: Arc::new(TestLlmProvider),
            config: Arc::new(config),
            github_webhook_secret: secret.to_owned(),
        }
    }

    async fn insert_workspace(pool: &PgPool, workspace_id: Uuid) {
        let name = format!("workspace-{workspace_id}");
        let result = sqlx::query("INSERT INTO workspaces (id, name, config) VALUES ($1, $2, $3)")
            .bind(workspace_id)
            .bind(name)
            .bind(json!({}))
            .execute(pool)
            .await;

        if let Err(error) = result {
            panic!("test workspace insert should succeed: {error}");
        }
    }

    fn signed_header(body: &[u8], secret: &str) -> String {
        let mut mac = match HmacSha256::new_from_slice(secret.as_bytes()) {
            Ok(mac) => mac,
            Err(error) => panic!("HMAC should accept test secret: {error}"),
        };
        mac.update(body);
        format!("sha256={}", hex::encode(mac.finalize().into_bytes()))
    }

    fn pull_request_body() -> String {
        json!({
            "sender": { "login": "octocat" },
            "pull_request": { "updated_at": Utc::now().to_rfc3339() }
        })
        .to_string()
    }

    fn request(
        body: String,
        workspace_id: Uuid,
        signature: Option<String>,
        event: &str,
    ) -> Request<Body> {
        let mut builder = Request::builder()
            .method(Method::POST)
            .uri("/v1/ingest/github")
            .header("content-type", "application/json")
            .header(EVENT_HEADER, event)
            .header(DELIVERY_HEADER, Uuid::now_v7().to_string())
            .header(WORKSPACE_HEADER, workspace_id.to_string());

        if let Some(signature) = signature {
            builder = builder.header(SIGNATURE_HEADER, signature);
        }

        match builder.body(Body::from(body)) {
            Ok(request) => request,
            Err(error) => panic!("test request should build: {error}"),
        }
    }

    #[sqlx::test(migrations = "../../migrations")]
    #[ignore = "requires live PostgreSQL and Redis from docker-compose.test.yml"]
    async fn missing_signature_header_returns_401(pool: PgPool) {
        let workspace_id = Uuid::now_v7();
        let app = ingestion_router().with_state(test_state(pool, "secret").await);
        let response = match app
            .oneshot(request(
                pull_request_body(),
                workspace_id,
                None,
                "pull_request",
            ))
            .await
        {
            Ok(response) => response,
            Err(error) => panic!("handler should respond: {error}"),
        };

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[sqlx::test(migrations = "../../migrations")]
    #[ignore = "requires live PostgreSQL and Redis from docker-compose.test.yml"]
    async fn invalid_signature_returns_401(pool: PgPool) {
        let workspace_id = Uuid::now_v7();
        let body = pull_request_body();
        let app = ingestion_router().with_state(test_state(pool, "secret").await);
        let response = match app
            .oneshot(request(
                body,
                workspace_id,
                Some("sha256=0000".to_owned()),
                "pull_request",
            ))
            .await
        {
            Ok(response) => response,
            Err(error) => panic!("handler should respond: {error}"),
        };

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[sqlx::test(migrations = "../../migrations")]
    #[ignore = "requires live PostgreSQL and Redis from docker-compose.test.yml"]
    async fn unknown_event_type_returns_400(pool: PgPool) {
        let workspace_id = Uuid::now_v7();
        insert_workspace(&pool, workspace_id).await;
        let body = pull_request_body();
        let signature = signed_header(body.as_bytes(), "secret");
        let app = ingestion_router().with_state(test_state(pool, "secret").await);
        let response = match app
            .oneshot(request(body, workspace_id, Some(signature), "deployment"))
            .await
        {
            Ok(response) => response,
            Err(error) => panic!("handler should respond: {error}"),
        };

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[sqlx::test(migrations = "../../migrations")]
    #[ignore = "requires live PostgreSQL and Redis from docker-compose.test.yml"]
    async fn valid_pull_request_returns_202(pool: PgPool) {
        let workspace_id = Uuid::now_v7();
        insert_workspace(&pool, workspace_id).await;
        let body = pull_request_body();
        let signature = signed_header(body.as_bytes(), "secret");
        let app = ingestion_router().with_state(test_state(pool, "secret").await);
        let response = match app
            .oneshot(request(body, workspace_id, Some(signature), "pull_request"))
            .await
        {
            Ok(response) => response,
            Err(error) => panic!("handler should respond: {error}"),
        };

        assert_eq!(response.status(), StatusCode::ACCEPTED);
    }
}
