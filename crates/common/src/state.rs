use std::sync::Arc;

use deadpool_redis::Pool as RedisPool;
use qdrant_client::Qdrant;
use sqlx::PgPool;
use tokio::sync::Semaphore;
use zeroize::Zeroizing;

use crate::{
    config::{
        AnthropicConfig, AppConfig, EmbeddingProviderKind, GeminiConfig, LlmProviderKind,
        OllamaConfig, OpenAiCompatibleConfig, OpenAiEmbeddingConfig, OpenAiLlmConfig,
    },
    models::WorkspaceConfig,
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
    pub app_secret_key: Arc<Zeroizing<String>>,
    /// Parsed `TRUSTED_PROXY_CIDRS` env var: `(network_addr, prefix_len)` pairs.
    /// Only peers whose address falls in one of these CIDRs are trusted to set
    /// `X-Forwarded-For`.  Empty = trust nobody (use direct peer IP only).
    pub trusted_proxy_cidrs: Arc<Vec<(std::net::IpAddr, u8)>>,
}

pub fn build_embedding_provider_for_workspace(
    config: &AppConfig,
    workspace_config: &WorkspaceConfig,
) -> Arc<dyn EmbeddingProvider> {
    let mut effective = config.clone();
    if let Some(provider) = workspace_config
        .embedding_provider
        .as_deref()
        .and_then(parse_embedding_provider_kind)
    {
        effective.embedding.provider = provider;
    }
    if let Some(model) = workspace_config.embedding_model.as_deref() {
        effective.embedding.model = model.to_owned();
        if effective.embedding.provider == EmbeddingProviderKind::Openai {
            effective.embedding.openai = Some(OpenAiEmbeddingConfig {
                model: model.to_owned(),
            });
        }
    }

    build_embedding_provider(&effective)
}

pub fn build_llm_provider_for_workspace(
    config: &AppConfig,
    workspace_config: &WorkspaceConfig,
) -> Arc<dyn LlmProvider> {
    let mut effective = config.clone();
    if let Some(provider) = workspace_config
        .llm_provider
        .as_deref()
        .and_then(parse_llm_provider_kind)
    {
        effective.llm.provider = provider;
    }
    if let Some(model) = workspace_config
        .llm_model
        .as_deref()
        .map(str::trim)
        .filter(|model| !model.is_empty())
    {
        effective.llm.model = model.to_owned();
        effective.llm.openai = Some(OpenAiLlmConfig {
            model: model.to_owned(),
        });
        effective.llm.anthropic = Some(AnthropicConfig {
            model: model.to_owned(),
        });
        if effective.llm.provider == LlmProviderKind::Gemini && effective.llm.gemini.is_none() {
            effective.llm.gemini = Some(GeminiConfig { api_key_env: None });
        }
    }
    if let Some(base_url) = workspace_config
        .llm_base_url
        .as_deref()
        .map(str::trim)
        .filter(|base_url| !base_url.is_empty())
    {
        effective.llm.base_url = Some(base_url.to_owned());
    }
    ensure_workspace_compatible_llm_config(&mut effective);
    if let Some(api_key_env) = workspace_config
        .llm_api_key_env
        .as_deref()
        .map(str::trim)
        .filter(|api_key_env| !api_key_env.is_empty())
    {
        apply_workspace_llm_api_key_env(&mut effective, api_key_env);
    }

    build_llm_provider(&effective)
}

fn ensure_workspace_compatible_llm_config(config: &mut AppConfig) {
    if matches!(
        config.llm.provider,
        LlmProviderKind::OpenaiCompatible
            | LlmProviderKind::Openrouter
            | LlmProviderKind::Huggingface
    ) && config.llm.openai_compatible.is_none()
    {
        config.llm.openai_compatible = Some(OpenAiCompatibleConfig {
            api_key_env: None,
            headers: Default::default(),
        });
    }
}

fn apply_workspace_llm_api_key_env(config: &mut AppConfig, api_key_env: &str) {
    let api_key_env = Some(api_key_env.to_owned());
    match config.llm.provider {
        LlmProviderKind::Ollama => {
            config.llm.ollama = Some(OllamaConfig { api_key_env });
        }
        LlmProviderKind::OpenaiCompatible
        | LlmProviderKind::Openrouter
        | LlmProviderKind::Huggingface => {
            let headers = config
                .llm
                .openai_compatible
                .as_ref()
                .map(|cfg| cfg.headers.clone())
                .unwrap_or_default();
            config.llm.openai_compatible = Some(OpenAiCompatibleConfig {
                api_key_env,
                headers,
            });
        }
        LlmProviderKind::Gemini => {
            config.llm.gemini = Some(GeminiConfig { api_key_env });
        }
        LlmProviderKind::Openai | LlmProviderKind::Anthropic => {
            tracing::warn!(
                provider = ?config.llm.provider,
                "workspace llm_api_key_env override is ignored for providers with fixed env vars"
            );
        }
    }
}

fn parse_embedding_provider_kind(value: &str) -> Option<EmbeddingProviderKind> {
    match value.trim().to_ascii_lowercase().as_str() {
        "fastembed" | "fast_embed" => Some(EmbeddingProviderKind::FastEmbed),
        "openai" => Some(EmbeddingProviderKind::Openai),
        other => {
            tracing::warn!(
                provider = other,
                "unknown workspace embedding provider override"
            );
            None
        }
    }
}

fn parse_llm_provider_kind(value: &str) -> Option<LlmProviderKind> {
    match value.trim().to_ascii_lowercase().as_str() {
        "ollama" => Some(LlmProviderKind::Ollama),
        "openai" => Some(LlmProviderKind::Openai),
        "anthropic" => Some(LlmProviderKind::Anthropic),
        "openai_compatible" | "openai-compatible" => Some(LlmProviderKind::OpenaiCompatible),
        "openrouter" => Some(LlmProviderKind::Openrouter),
        "huggingface" | "hugging_face" => Some(LlmProviderKind::Huggingface),
        "gemini" => Some(LlmProviderKind::Gemini),
        other => {
            tracing::warn!(provider = other, "unknown workspace LLM provider override");
            None
        }
    }
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
                    LlmProviderKind::OpenaiCompatible => "https://api.openai.com/v1".to_owned(),
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
                config.llm.timeout_secs,
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
