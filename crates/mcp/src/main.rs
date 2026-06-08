use std::sync::Arc;

use common::{
    build_embedding_provider, build_llm_provider, config::AppConfig,
    crypto::app_secret_key_from_env, db::connect_pool, telemetry::init_telemetry, AppState,
};
use mcp::{
    server::{McpServer, RuntimeBackend},
    transport, MCP_PROTOCOL_VERSION,
};
use qdrant_client::Qdrant;
use tokio::sync::Semaphore;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config_path = std::env::var("CONFIG_PATH").unwrap_or_else(|_| "config.toml".to_owned());
    let config = AppConfig::from_path(config_path)?;
    let _telemetry_guard = init_telemetry(&config.telemetry)?;
    let state = build_state(config).await?;
    let server = Arc::new(McpServer::new(RuntimeBackend::new(state)));
    let transport_mode = std::env::var("MCP_TRANSPORT").unwrap_or_else(|_| "stdio".to_owned());

    match transport_mode.as_str() {
        "stdio" => {
            tracing::info!(
                transport = "stdio",
                protocol_version = MCP_PROTOCOL_VERSION,
                "starting MemoryOps MCP server"
            );
            transport::stdio::run(server).await?;
        }
        "http" => {
            let port = transport::http_streamable::port_from_env();
            tracing::info!(
                transport = "http",
                port,
                protocol_version = MCP_PROTOCOL_VERSION,
                "starting MemoryOps MCP server"
            );
            transport::http_streamable::run(server).await?;
        }
        "sse" => {
            #[allow(deprecated)]
            {
                let port = transport::sse::port_from_env();
                tracing::info!(
                    transport = "sse",
                    port,
                    protocol_version = MCP_PROTOCOL_VERSION,
                    "starting MemoryOps MCP server"
                );
                transport::sse::run(server).await?;
            }
        }
        other => anyhow::bail!("unsupported MCP_TRANSPORT: {other}"),
    }

    Ok(())
}

async fn build_state(config: AppConfig) -> anyhow::Result<AppState> {
    let database_url =
        std::env::var("DATABASE_URL").map_err(|_| anyhow::anyhow!("DATABASE_URL not set"))?;
    let redis_url = std::env::var("REDIS_URL").map_err(|_| anyhow::anyhow!("REDIS_URL not set"))?;
    let qdrant_url =
        std::env::var("QDRANT_URL").map_err(|_| anyhow::anyhow!("QDRANT_URL not set"))?;
    let app_secret_key = app_secret_key_from_env()
        .map_err(|_| anyhow::anyhow!("APP_SECRET_KEY is missing or invalid -- cannot start"))?;

    let db = connect_pool(&database_url, &config.database).await?;
    let redis = {
        let cfg = deadpool_redis::Config::from_url(&redis_url);
        cfg.create_pool(Some(deadpool_redis::Runtime::Tokio1))
            .map_err(|error| anyhow::anyhow!("failed to create Redis pool: {error}"))?
    };
    let qdrant = Qdrant::from_url(&qdrant_url).build()?;

    Ok(AppState {
        db,
        redis,
        qdrant,
        processor_semaphore: Arc::new(Semaphore::new(
            config.processor.fast_path_concurrency.max(1),
        )),
        embedding_provider: build_embedding_provider(&config),
        llm_provider: build_llm_provider(&config),
        config: Arc::new(config),
        app_secret_key: Arc::new(app_secret_key),
        trusted_proxy_cidrs: Arc::new(Vec::new()),
    })
}
