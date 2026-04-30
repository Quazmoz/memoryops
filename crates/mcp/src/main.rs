use std::{sync::Arc, time::Duration};

use common::{
    config::{AppConfig, EmbeddingProviderKind, LlmProviderKind},
    providers::{
        AnthropicProvider, EmbeddingProvider, FastEmbedProvider, LlmProvider, OllamaProvider,
        OpenAIEmbedProvider, OpenAIProvider,
    },
    telemetry::init_telemetry,
    AppState,
};
use mcp::{
    server::{McpServer, RuntimeBackend},
    transport, MCP_PROTOCOL_VERSION,
};
use qdrant_client::Qdrant;
use redis::aio::ConnectionManager;
use sqlx::postgres::PgPoolOptions;
use tokio::sync::Semaphore;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config_path = std::env::var("CONFIG_PATH").unwrap_or_else(|_| "config.toml".to_owned());
    let config = AppConfig::from_path(config_path)?;
    let _telemetry_guard = init_telemetry(&config.telemetry)?;
    let state = build_state(config).await?;
    let server = Arc::new(McpServer::new(RuntimeBackend::new(state)));
    let transport_mode = std::env::var("MCP_TRANSPORT").unwrap_or_else(|_| "sse".to_owned());

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
        processor_semaphore: Arc::new(Semaphore::new(
            usize::try_from(config.database.max_connections).unwrap_or(10),
        )),
        embedding_provider: build_embedding_provider(&config),
        llm_provider: build_llm_provider(&config),
        config: Arc::new(config),
        github_webhook_secret: std::env::var("GITHUB_WEBHOOK_SECRET").unwrap_or_default(),
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
