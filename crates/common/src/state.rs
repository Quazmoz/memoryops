use std::sync::Arc;

use deadpool_redis::Pool as RedisPool;
use qdrant_client::Qdrant;
use sqlx::PgPool;
use tokio::sync::Semaphore;

use crate::{
    config::{AppConfig, EmbeddingProviderKind, LlmProviderKind},
    providers::{
        AnthropicProvider, EmbeddingProvider, FastEmbedProvider, GeminiProvider, LlmProvider,
        OllamaProvider, OpenAIEmbedProvider, OpenAIProvider, OpenAiCompatibleProvider,
    },
};

#[derive(Clone)]
pub struct AppState {
    pub db: PgPool,
    pub redis: RedisPool,
    pub qdrant: Qdrant,
    pub processor_semaphore: Arc<Semaphore>,
    pub embedding_provider: Arc<dyn EmbeddingProvider>,
    pub llm_provider: Arc<dyn LlmProvider>,
    pub config: Arc<AppConfig>,
    pub github_webhook_secret: String,
    /// Parsed `TRUSTED_PROXY_CIDRS` env var: `(network_addr, prefix_len)` pairs.
    /// Only peers whose address falls in one of these CIDRs are trusted to set
    /// `X-Forwarded-For`.  Empty = trust nobody (use direct peer IP only).
    pub trusted_proxy_cidrs: Arc<Vec<(std::net::IpAddr, u8)>>,
}

pub fn build_embedding_provider(config: &AppConfig) -> Arc<dyn EmbeddingProvider> {
    match config.embedding.provider {
        EmbeddingProviderKind::FastEmbed => {
            Arc::new(FastEmbedProvider::new(&config.embedding.model))
        }
        EmbeddingProviderKind::Openai => {
            if config.embedding.openai.is_none() {
                tracing::warn!(
                    "provider-specific config block is None; falling back to config.llm.model"
                );
            }
            let model = config
                .embedding
                .openai
                .as_ref()
                .map(|openai| openai.model.as_str())
                .unwrap_or(&config.embedding.model);
            Arc::new(OpenAIEmbedProvider::new(
                model,
                std::env::var("OPENAI_API_KEY").ok(),
                config.llm.timeout_secs,
            ))
        }
    }
}

pub fn build_llm_provider(config: &AppConfig) -> Arc<dyn LlmProvider> {
    match config.llm.provider {
        LlmProviderKind::Ollama => {
            // Resolve an optional Bearer token for cloud/hosted Ollama instances.
            // Local Ollama doesn't need one; the resolved key will be None in that case.
            let api_key = config
                .llm
                .ollama
                .as_ref()
                .and_then(|ollama_cfg| ollama_cfg.resolve_api_key());
            Arc::new(OllamaProvider::new(
                config.llm.base_url.as_deref().unwrap_or(""),
                &config.llm.model,
                config.llm.timeout_secs,
                api_key,
            ))
        }
        LlmProviderKind::Openai => {
            if config.llm.openai.is_none() {
                tracing::warn!(
                    "provider-specific config block is None; falling back to config.llm.model"
                );
            }
            let model = config
                .llm
                .openai
                .as_ref()
                .map(|openai| openai.model.as_str())
                .unwrap_or(&config.llm.model);
            Arc::new(OpenAIProvider::new(
                model,
                std::env::var("OPENAI_API_KEY").ok(),
                config.llm.timeout_secs,
            ))
        }
        LlmProviderKind::Anthropic => {
            if config.llm.anthropic.is_none() {
                tracing::warn!(
                    "provider-specific config block is None; falling back to config.llm.model"
                );
            }
            let model = config
                .llm
                .anthropic
                .as_ref()
                .map(|anthropic| anthropic.model.as_str())
                .unwrap_or(&config.llm.model);
            Arc::new(AnthropicProvider::new(
                model,
                std::env::var("ANTHROPIC_API_KEY").ok(),
                config.llm.timeout_secs,
            ))
        }
        LlmProviderKind::OpenaiCompatible
        | LlmProviderKind::Openrouter
        | LlmProviderKind::Huggingface => {
            let compat = config.llm.openai_compatible.as_ref();
            let api_key = compat.and_then(|cfg| cfg.resolve_api_key());
            let headers = compat.map(|cfg| cfg.headers.clone()).unwrap_or_default();
            let configured_base_url = config.llm.base_url.as_deref().unwrap_or("");
            let base_url = if configured_base_url.trim().is_empty() {
                match config.llm.provider {
                    LlmProviderKind::Openrouter => "https://openrouter.ai/api/v1".to_owned(),
                    LlmProviderKind::Huggingface => "https://router.huggingface.co/v1".to_owned(),
                    _ => configured_base_url.to_owned(),
                }
            } else {
                configured_base_url.to_owned()
            };
            Arc::new(OpenAiCompatibleProvider::new(
                base_url,
                &config.llm.model,
                api_key,
                headers,
            ))
        }
        LlmProviderKind::Gemini => {
            if config.llm.gemini.is_none() {
                tracing::warn!(
                    "[llm.gemini] config block is missing; GEMINI_API_KEY env var will be used directly"
                );
            }
            let api_key = config
                .llm
                .gemini
                .as_ref()
                .and_then(|gemini_cfg| gemini_cfg.resolve_api_key())
                // Fall back to the conventional env var name if no config block present.
                .or_else(|| {
                    std::env::var("GEMINI_API_KEY")
                        .ok()
                        .filter(|v| !v.trim().is_empty())
                });
            let model = config
                .llm
                .gemini
                .as_ref()
                .map(|_| config.llm.model.as_str())
                .unwrap_or(&config.llm.model);
            Arc::new(GeminiProvider::new(model, api_key, config.llm.timeout_secs))
        }
    }
}
