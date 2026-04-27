use std::time::Duration;

use axum::{http::StatusCode, routing::get, Json, Router};
use common::{config::AppConfig, telemetry::init_telemetry};
use qdrant_client::Qdrant;
use serde_json::{json, Value};
use sqlx::postgres::PgPoolOptions;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config_path = std::env::var("CONFIG_PATH").unwrap_or_else(|_| "config.toml".to_owned());
    let config = AppConfig::from_path(config_path)?;
    let _telemetry_guard = init_telemetry(&config.telemetry)?;

    let address = format!("{}:{}", config.server.host, config.server.port);
    let listener = tokio::net::TcpListener::bind(&address).await?;

    tracing::info!(%address, "starting MemoryOps API");
    axum::serve(listener, router()).await?;

    Ok(())
}

fn router() -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/health/ready", get(readiness))
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
