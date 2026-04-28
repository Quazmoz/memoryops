use std::{sync::Arc, time::Duration};

use axum::{http::StatusCode, middleware as axum_middleware, routing::get, Json, Router};
use common::{
    config::{AppConfig, EmbeddingProviderKind, LlmProviderKind},
    providers::{
        AnthropicProvider, EmbeddingProvider, FastEmbedProvider, LlmProvider, OllamaProvider,
        OpenAIEmbedProvider, OpenAIProvider,
    },
    telemetry::init_telemetry,
    AppState,
};
use qdrant_client::Qdrant;
use redis::aio::ConnectionManager;
use retrieval::retrieval_router;
use serde_json::{json, Value};
use sqlx::postgres::PgPoolOptions;

mod handlers;
mod middleware;
mod security;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config_path = std::env::var("CONFIG_PATH").unwrap_or_else(|_| "config.toml".to_owned());
    let config = AppConfig::from_path(config_path)?;
    let _telemetry_guard = init_telemetry(&config.telemetry)?;
    let state = build_state(config.clone()).await?;
    processor::start_workers(state.clone()).await?;
    tokio::spawn(processor::scheduler::run_scheduler(Arc::new(state.clone())));

    let address = format!("{}:{}", config.server.host, config.server.port);
    let listener = tokio::net::TcpListener::bind(&address).await?;

    tracing::info!(%address, "starting MemoryOps API");
    axum::serve(listener, router(state)).await?;

    Ok(())
}

fn router(state: AppState) -> Router {
    let ingestion_router = ingestion::ingestion_router().layer(
        axum_middleware::from_fn_with_state(state.clone(), middleware::rate_limit::rate_limit),
    );
    let retrieval_router = retrieval_router()
        .layer(axum_middleware::from_fn_with_state(
            state.clone(),
            middleware::rate_limit::rate_limit,
        ))
        .layer(axum_middleware::from_fn_with_state(
            state.clone(),
            middleware::auth::require_api_key,
        ));
    let protected_api_router = handlers::protected_router()
        .layer(axum_middleware::from_fn_with_state(
            state.clone(),
            middleware::rate_limit::rate_limit,
        ))
        .layer(axum_middleware::from_fn_with_state(
            state.clone(),
            middleware::auth::require_api_key,
        ));

    Router::new()
        .route("/health", get(health))
        .route("/health/ready", get(readiness))
        .merge(handlers::bootstrap_router())
        .merge(ingestion_router)
        .merge(retrieval_router)
        .merge(protected_api_router)
        .with_state(state)
}

async fn build_state(config: AppConfig) -> anyhow::Result<AppState> {
    let database_url =
        std::env::var("DATABASE_URL").map_err(|_| anyhow::anyhow!("DATABASE_URL not set"))?;
    let redis_url = std::env::var("REDIS_URL").map_err(|_| anyhow::anyhow!("REDIS_URL not set"))?;
    let qdrant_url =
        std::env::var("QDRANT_URL").map_err(|_| anyhow::anyhow!("QDRANT_URL not set"))?;
    let github_webhook_secret = std::env::var("GITHUB_WEBHOOK_SECRET")
        .map_err(|_| anyhow::anyhow!("GITHUB_WEBHOOK_SECRET not set"))?;

    let db = PgPoolOptions::new()
        .max_connections(config.database.max_connections)
        .min_connections(config.database.min_connections)
        .acquire_timeout(Duration::from_secs(config.database.connect_timeout_secs))
        .connect(&database_url)
        .await?;
    let redis_client = redis::Client::open(redis_url)?;
    let redis = ConnectionManager::new(redis_client).await?;
    let qdrant = Qdrant::from_url(&qdrant_url).build()?;

    let embedding_provider = build_embedding_provider(&config);
    let llm_provider = build_llm_provider(&config);

    Ok(AppState {
        db,
        redis,
        qdrant,
        embedding_provider,
        llm_provider,
        config: Arc::new(config),
        github_webhook_secret,
    })
}

fn build_embedding_provider(config: &AppConfig) -> Arc<dyn EmbeddingProvider> {
    match config.embedding.provider {
        EmbeddingProviderKind::FastEmbed => {
            Arc::new(FastEmbedProvider::new(&config.embedding.model))
        }
        EmbeddingProviderKind::Openai => {
            let model = config
                .embedding
                .openai
                .as_ref()
                .map(|openai| openai.model.as_str())
                .unwrap_or(&config.embedding.model);
            Arc::new(OpenAIEmbedProvider::new(
                model,
                std::env::var("OPENAI_API_KEY").ok(),
            ))
        }
    }
}

fn build_llm_provider(config: &AppConfig) -> Arc<dyn LlmProvider> {
    match config.llm.provider {
        LlmProviderKind::Ollama => Arc::new(OllamaProvider::new(
            &config.llm.base_url,
            &config.llm.model,
            config.llm.timeout_secs,
        )),
        LlmProviderKind::Openai => {
            let model = config
                .llm
                .openai
                .as_ref()
                .map(|openai| openai.model.as_str())
                .unwrap_or(&config.llm.model);
            Arc::new(OpenAIProvider::new(
                model,
                std::env::var("OPENAI_API_KEY").ok(),
            ))
        }
        LlmProviderKind::Anthropic => {
            let model = config
                .llm
                .anthropic
                .as_ref()
                .map(|anthropic| anthropic.model.as_str())
                .unwrap_or(&config.llm.model);
            Arc::new(AnthropicProvider::new(
                model,
                std::env::var("ANTHROPIC_API_KEY").ok(),
            ))
        }
    }
}

async fn health() -> Json<Value> {
    Json(json!({ "status": "ok" }))
}

async fn readiness() -> (StatusCode, Json<Value>) {
    let (database, redis, qdrant) = tokio::join!(check_database(), check_redis(), check_qdrant());
    let ready = database.is_ready() && redis.is_ready() && qdrant.is_ready();
    let status = if ready {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };

    (
        status,
        Json(json!({
            "status": if ready { "ok" } else { "unavailable" },
            "checks": {
                "database": database.as_str(),
                "redis": redis.as_str(),
                "qdrant": qdrant.as_str()
            }
        })),
    )
}

#[derive(Debug, Clone, Copy)]
enum DependencyStatus {
    Ok,
    MissingConfig,
    Unavailable,
}

impl DependencyStatus {
    fn is_ready(self) -> bool {
        matches!(self, DependencyStatus::Ok)
    }

    fn as_str(self) -> &'static str {
        match self {
            DependencyStatus::Ok => "ok",
            DependencyStatus::MissingConfig => "missing_config",
            DependencyStatus::Unavailable => "unavailable",
        }
    }
}

async fn check_database() -> DependencyStatus {
    let Ok(database_url) = std::env::var("DATABASE_URL") else {
        return DependencyStatus::MissingConfig;
    };

    match PgPoolOptions::new()
        .max_connections(1)
        .acquire_timeout(Duration::from_secs(2))
        .connect(&database_url)
        .await
    {
        Ok(pool) => {
            pool.close().await;
            DependencyStatus::Ok
        }
        Err(_) => DependencyStatus::Unavailable,
    }
}

async fn check_redis() -> DependencyStatus {
    let Ok(redis_url) = std::env::var("REDIS_URL") else {
        return DependencyStatus::MissingConfig;
    };

    let Ok(client) = redis::Client::open(redis_url) else {
        return DependencyStatus::Unavailable;
    };

    let Ok(mut connection) = client.get_multiplexed_async_connection().await else {
        return DependencyStatus::Unavailable;
    };

    match redis::cmd("PING")
        .query_async::<String>(&mut connection)
        .await
    {
        Ok(_) => DependencyStatus::Ok,
        Err(_) => DependencyStatus::Unavailable,
    }
}

async fn check_qdrant() -> DependencyStatus {
    let Ok(qdrant_url) = std::env::var("QDRANT_URL") else {
        return DependencyStatus::MissingConfig;
    };

    let Ok(client) = Qdrant::from_url(&qdrant_url).build() else {
        return DependencyStatus::Unavailable;
    };

    match client.health_check().await {
        Ok(_) => DependencyStatus::Ok,
        Err(_) => DependencyStatus::Unavailable,
    }
}

#[cfg(test)]
mod tests {
    use std::{
        sync::Arc,
        time::{SystemTime, UNIX_EPOCH},
    };

    use axum::{
        body::{to_bytes, Body},
        http::{Method, Request, StatusCode},
    };
    use chrono::Utc;
    use common::models::MemoryType;
    use redis::aio::ConnectionManager;
    use serde_json::{json, Value};
    use sqlx::PgPool;
    use tower::ServiceExt;
    use uuid::Uuid;

    use super::*;

    async fn test_state(pool: PgPool) -> AppState {
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
        let config = match AppConfig::from_toml_str(include_str!("../../../config.toml")) {
            Ok(config) => config,
            Err(error) => panic!("checked-in config should parse: {error}"),
        };

        AppState {
            db: pool,
            redis,
            qdrant,
            embedding_provider: Arc::new(FastEmbedProvider::new("test-embedding")),
            llm_provider: Arc::new(OllamaProvider::new("http://127.0.0.1:9", "test-llm", 1)),
            config: Arc::new(config),
            github_webhook_secret: "test-secret".to_owned(),
        }
    }

    async fn insert_workspace(pool: &PgPool) -> Uuid {
        let workspace_id = Uuid::now_v7();
        let result = sqlx::query("INSERT INTO workspaces (id, name, config) VALUES ($1, $2, $3)")
            .bind(workspace_id)
            .bind(format!("workspace-{workspace_id}"))
            .bind(json!({}))
            .execute(pool)
            .await;

        if let Err(error) = result {
            panic!("test workspace insert should succeed: {error}");
        }

        workspace_id
    }

    async fn insert_api_key(pool: &PgPool, workspace_id: Uuid, revoked: bool) -> String {
        let key_id = Uuid::now_v7();
        let (plaintext, prefix) = security::generate_api_key(workspace_id);
        let key_hash = match security::hash_secret(&plaintext) {
            Ok(hash) => hash,
            Err(error) => panic!("test key hash should be generated: {error}"),
        };
        let revoked_at = if revoked { Some(Utc::now()) } else { None };
        let result = sqlx::query(
            r#"
            INSERT INTO api_keys (id, workspace_id, name, key_hash, prefix, revoked, revoked_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            "#,
        )
        .bind(key_id)
        .bind(workspace_id)
        .bind("test key")
        .bind(key_hash)
        .bind(prefix)
        .bind(revoked)
        .bind(revoked_at)
        .execute(pool)
        .await;

        if let Err(error) = result {
            panic!("test API key insert should succeed: {error}");
        }

        plaintext
    }

    async fn insert_memory(pool: &PgPool, workspace_id: Uuid, content: &str) -> Uuid {
        let memory_id = Uuid::now_v7();
        let result = sqlx::query(
            r#"
            INSERT INTO memory_units (
                id,
                workspace_id,
                scope,
                memory_type,
                content,
                entities,
                importance_score,
                tags
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            "#,
        )
        .bind(memory_id)
        .bind(workspace_id)
        .bind(json!({
            "workspace_id": workspace_id,
            "agent_id": null,
            "user_id": null,
            "repo": "Quazmoz/memoryops"
        }))
        .bind(MemoryType::Episodic)
        .bind(content)
        .bind(json!([]))
        .bind(0.9_f32)
        .bind(Vec::<String>::new())
        .execute(pool)
        .await;

        if let Err(error) = result {
            panic!("test memory insert should succeed: {error}");
        }

        memory_id
    }

    fn request(method: Method, uri: String, api_key: Option<&str>, body: Value) -> Request<Body> {
        let mut builder = Request::builder()
            .method(method)
            .uri(uri)
            .header("content-type", "application/json");

        if let Some(api_key) = api_key {
            builder = builder.header("x-api-key", api_key);
        }

        match builder.body(Body::from(body.to_string())) {
            Ok(request) => request,
            Err(error) => panic!("test request should build: {error}"),
        }
    }

    async fn response_json(response: axum::response::Response) -> Value {
        let bytes = match to_bytes(response.into_body(), usize::MAX).await {
            Ok(bytes) => bytes,
            Err(error) => panic!("response body should be readable: {error}"),
        };
        match serde_json::from_slice::<Value>(&bytes) {
            Ok(value) => value,
            Err(error) => panic!("response body should be JSON: {error}"),
        }
    }

    #[sqlx::test(migrations = "../../migrations")]
    #[ignore = "requires live PostgreSQL and Redis from docker-compose.test.yml"]
    async fn bootstrap_flow_creates_key_and_lists_memory(pool: PgPool) {
        let app = router(test_state(pool).await);
        let workspace_name = format!("workspace-{}", Uuid::now_v7());
        let response = match app
            .clone()
            .oneshot(request(
                Method::POST,
                "/v1/workspaces".to_owned(),
                None,
                json!({ "name": workspace_name }),
            ))
            .await
        {
            Ok(response) => response,
            Err(error) => panic!("workspace request should respond: {error}"),
        };
        assert_eq!(response.status(), StatusCode::OK);
        let body = response_json(response).await;
        let workspace_id = match body.get("workspace_id").and_then(Value::as_str) {
            Some(raw) => match Uuid::parse_str(raw) {
                Ok(workspace_id) => workspace_id,
                Err(error) => panic!("workspace_id should be a UUID: {error}"),
            },
            None => panic!("workspace response should include workspace_id"),
        };

        let response = match app
            .clone()
            .oneshot(request(
                Method::POST,
                format!("/v1/workspaces/{workspace_id}/keys"),
                None,
                json!({ "name": "bootstrap" }),
            ))
            .await
        {
            Ok(response) => response,
            Err(error) => panic!("key request should respond: {error}"),
        };
        assert_eq!(response.status(), StatusCode::OK);
        let body = response_json(response).await;
        let api_key = match body.get("key").and_then(Value::as_str) {
            Some(key) => key.to_owned(),
            None => panic!("key response should expose plaintext key once"),
        };

        let response = match app
            .oneshot(request(
                Method::GET,
                "/v1/memory".to_owned(),
                Some(&api_key),
                json!(null),
            ))
            .await
        {
            Ok(response) => response,
            Err(error) => panic!("memory list should respond: {error}"),
        };

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[sqlx::test(migrations = "../../migrations")]
    #[ignore = "requires live PostgreSQL and Redis from docker-compose.test.yml"]
    async fn missing_key_returns_401(pool: PgPool) {
        let app = router(test_state(pool).await);
        let response = match app
            .oneshot(request(
                Method::GET,
                "/v1/memory".to_owned(),
                None,
                json!(null),
            ))
            .await
        {
            Ok(response) => response,
            Err(error) => panic!("request should respond: {error}"),
        };

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[sqlx::test(migrations = "../../migrations")]
    #[ignore = "requires live PostgreSQL and Redis from docker-compose.test.yml"]
    async fn retrieve_packs_memory_within_budget_and_persists_trace(pool: PgPool) {
        let workspace_id = insert_workspace(&pool).await;
        let api_key = insert_api_key(&pool, workspace_id, false).await;
        let memory_id = insert_memory(&pool, workspace_id, "alpha").await;
        let app = router(test_state(pool).await);

        let response = match app
            .clone()
            .oneshot(request(
                Method::POST,
                "/v1/retrieve".to_owned(),
                Some(&api_key),
                json!({
                    "query": "alpha",
                    "workspace_id": workspace_id,
                    "token_budget": 1,
                    "mode": "keyword",
                    "include_trace": false
                }),
            ))
            .await
        {
            Ok(response) => response,
            Err(error) => panic!("retrieve request should respond: {error}"),
        };
        assert_eq!(response.status(), StatusCode::OK);
        let body = response_json(response).await;
        let total_tokens = body
            .get("total_tokens")
            .and_then(Value::as_u64)
            .unwrap_or(u64::MAX);
        assert!(total_tokens <= 1);
        assert!(body.get("trace").is_some_and(Value::is_null));
        let memory_id_text = memory_id.to_string();
        assert_eq!(
            body.get("memories")
                .and_then(Value::as_array)
                .and_then(|memories| memories.first())
                .and_then(|memory| memory.get("id"))
                .and_then(Value::as_str),
            Some(memory_id_text.as_str())
        );
        let query_id = match body.get("query_id").and_then(Value::as_str) {
            Some(raw) => match Uuid::parse_str(raw) {
                Ok(query_id) => query_id,
                Err(error) => panic!("query_id should be a UUID: {error}"),
            },
            None => panic!("retrieve response should include query_id"),
        };

        let response = match app
            .oneshot(request(
                Method::GET,
                format!("/v1/retrieve/trace/{query_id}"),
                Some(&api_key),
                json!(null),
            ))
            .await
        {
            Ok(response) => response,
            Err(error) => panic!("trace request should respond: {error}"),
        };
        assert_eq!(response.status(), StatusCode::OK);
        let trace = response_json(response).await;
        let query_id_text = query_id.to_string();
        assert_eq!(
            trace.get("query_id").and_then(Value::as_str),
            Some(query_id_text.as_str())
        );
    }

    #[sqlx::test(migrations = "../../migrations")]
    #[ignore = "requires live PostgreSQL and Redis from docker-compose.test.yml"]
    async fn revoked_key_returns_401(pool: PgPool) {
        let workspace_id = insert_workspace(&pool).await;
        let api_key = insert_api_key(&pool, workspace_id, true).await;
        let app = router(test_state(pool).await);
        let response = match app
            .oneshot(request(
                Method::GET,
                "/v1/memory".to_owned(),
                Some(&api_key),
                json!(null),
            ))
            .await
        {
            Ok(response) => response,
            Err(error) => panic!("request should respond: {error}"),
        };

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[sqlx::test(migrations = "../../migrations")]
    #[ignore = "requires live PostgreSQL and Redis from docker-compose.test.yml"]
    async fn wrong_workspace_key_returns_403(pool: PgPool) {
        let key_workspace_id = insert_workspace(&pool).await;
        let target_workspace_id = insert_workspace(&pool).await;
        let api_key = insert_api_key(&pool, key_workspace_id, false).await;
        let app = router(test_state(pool).await);
        let response = match app
            .oneshot(request(
                Method::GET,
                format!("/v1/workspaces/{target_workspace_id}"),
                Some(&api_key),
                json!(null),
            ))
            .await
        {
            Ok(response) => response,
            Err(error) => panic!("request should respond: {error}"),
        };

        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }

    #[sqlx::test(migrations = "../../migrations")]
    #[ignore = "requires live PostgreSQL and Redis from docker-compose.test.yml"]
    async fn workspace_config_rejects_zero_decay_half_life(pool: PgPool) {
        let workspace_id = insert_workspace(&pool).await;
        let api_key = insert_api_key(&pool, workspace_id, false).await;
        let app = router(test_state(pool).await);
        let response = match app
            .oneshot(request(
                Method::PATCH,
                format!("/v1/workspaces/{workspace_id}/config"),
                Some(&api_key),
                json!({ "decay_half_life_days": 0 }),
            ))
            .await
        {
            Ok(response) => response,
            Err(error) => panic!("request should respond: {error}"),
        };

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[sqlx::test(migrations = "../../migrations")]
    #[ignore = "requires live PostgreSQL and Redis from docker-compose.test.yml"]
    async fn workspace_config_rejects_high_pruning_threshold(pool: PgPool) {
        let workspace_id = insert_workspace(&pool).await;
        let api_key = insert_api_key(&pool, workspace_id, false).await;
        let app = router(test_state(pool).await);
        let response = match app
            .oneshot(request(
                Method::PATCH,
                format!("/v1/workspaces/{workspace_id}/config"),
                Some(&api_key),
                json!({ "pruning_threshold": 0.99 }),
            ))
            .await
        {
            Ok(response) => response,
            Err(error) => panic!("request should respond: {error}"),
        };

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[sqlx::test(migrations = "../../migrations")]
    #[ignore = "requires live PostgreSQL and Redis from docker-compose.test.yml"]
    async fn workspace_config_accepts_lifecycle_values(pool: PgPool) {
        let workspace_id = insert_workspace(&pool).await;
        let api_key = insert_api_key(&pool, workspace_id, false).await;
        let app = router(test_state(pool).await);
        let response = match app
            .oneshot(request(
                Method::PATCH,
                format!("/v1/workspaces/{workspace_id}/config"),
                Some(&api_key),
                json!({
                    "decay_half_life_days": 90,
                    "pruning_threshold": 0.15
                }),
            ))
            .await
        {
            Ok(response) => response,
            Err(error) => panic!("request should respond: {error}"),
        };

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[sqlx::test(migrations = "../../migrations")]
    #[ignore = "requires live PostgreSQL and Redis from docker-compose.test.yml"]
    async fn rate_limit_exceeded_returns_429(pool: PgPool) {
        let workspace_id = insert_workspace(&pool).await;
        let api_key = insert_api_key(&pool, workspace_id, false).await;
        let state = test_state(pool).await;
        seed_rate_limit(&state.redis, workspace_id).await;
        let app = router(state);
        let response = match app
            .oneshot(request(
                Method::GET,
                "/v1/memory".to_owned(),
                Some(&api_key),
                json!(null),
            ))
            .await
        {
            Ok(response) => response,
            Err(error) => panic!("request should respond: {error}"),
        };

        assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
    }

    async fn seed_rate_limit(redis: &ConnectionManager, workspace_id: Uuid) {
        let now = match SystemTime::now().duration_since(UNIX_EPOCH) {
            Ok(duration) => i64::try_from(duration.as_secs()).unwrap_or(0),
            Err(error) => panic!("system time should be after epoch: {error}"),
        };
        let window_start = now - (now % 60);
        let mut connection = redis.clone();
        for window in [window_start, window_start + 60] {
            let key = format!("rate:{workspace_id}:memory:{window}");
            let result = redis::cmd("SET")
                .arg(key)
                .arg(120_i64)
                .query_async::<()>(&mut connection)
                .await;
            if let Err(error) = result {
                panic!("rate limit seed should succeed: {error}");
            }
        }
    }
}
