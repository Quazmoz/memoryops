use std::{future::Future, pin::Pin, sync::Arc, time::Duration};

use axum::{http::StatusCode, routing::get, Json, Router};
use common::{
    config::AppConfig,
    providers::{EmbeddingProvider, LlmProvider},
    telemetry::init_telemetry,
    AppState, ProviderError,
};
use qdrant_client::Qdrant;
use redis::aio::ConnectionManager;
use serde_json::{json, Value};
use sqlx::postgres::PgPoolOptions;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config_path = std::env::var("CONFIG_PATH").unwrap_or_else(|_| "config.toml".to_owned());
    let config = AppConfig::from_path(config_path)?;
    let _telemetry_guard = init_telemetry(&config.telemetry)?;
    let state = build_state(config.clone()).await?;
    processor::start_workers(state.clone()).await?;

    let address = format!("{}:{}", config.server.host, config.server.port);
    let listener = tokio::net::TcpListener::bind(&address).await?;

    tracing::info!(%address, "starting MemoryOps API");
    axum::serve(listener, router(state)).await?;

    Ok(())
}

fn router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/health/ready", get(readiness))
        .merge(ingestion::ingestion_router())
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

    Ok(AppState {
        db,
        redis,
        qdrant,
        embedding_provider: Arc::new(NotConfiguredEmbeddingProvider),
        llm_provider: Arc::new(NotConfiguredLlmProvider),
        config: Arc::new(config),
        github_webhook_secret,
    })
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

struct NotConfiguredEmbeddingProvider;

impl EmbeddingProvider for NotConfiguredEmbeddingProvider {
    fn embed<'life0, 'life1, 'async_trait>(
        &'life0 self,
        _text: &'life1 str,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<f32>, ProviderError>> + Send + 'async_trait>>
    where
        'life0: 'async_trait,
        'life1: 'async_trait,
        Self: Sync + 'async_trait,
    {
        Box::pin(async { Err(ProviderError::NotConfigured) })
    }

    fn embed_batch<'life0, 'life1, 'life2, 'async_trait>(
        &'life0 self,
        _texts: &'life1 [&'life2 str],
    ) -> Pin<Box<dyn Future<Output = Result<Vec<Vec<f32>>, ProviderError>> + Send + 'async_trait>>
    where
        'life0: 'async_trait,
        'life1: 'async_trait,
        'life2: 'async_trait,
        Self: Sync + 'async_trait,
    {
        Box::pin(async { Err(ProviderError::NotConfigured) })
    }

    fn dimensions(&self) -> usize {
        0
    }

    fn model_name(&self) -> &str {
        "not-configured"
    }
}

struct NotConfiguredLlmProvider;

impl LlmProvider for NotConfiguredLlmProvider {
    fn complete<'life0, 'life1, 'async_trait>(
        &'life0 self,
        _prompt: &'life1 str,
    ) -> Pin<Box<dyn Future<Output = Result<String, ProviderError>> + Send + 'async_trait>>
    where
        'life0: 'async_trait,
        'life1: 'async_trait,
        Self: Sync + 'async_trait,
    {
        Box::pin(async { Err(ProviderError::NotConfigured) })
    }

    fn summarize<'life0, 'life1, 'async_trait>(
        &'life0 self,
        _text: &'life1 str,
        _max_tokens: usize,
    ) -> Pin<Box<dyn Future<Output = Result<String, ProviderError>> + Send + 'async_trait>>
    where
        'life0: 'async_trait,
        'life1: 'async_trait,
        Self: Sync + 'async_trait,
    {
        Box::pin(async { Err(ProviderError::NotConfigured) })
    }
}
