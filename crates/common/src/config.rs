use std::{fs, path::Path};

use serde::Deserialize;

use crate::error::ConfigError;

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AppConfig {
    pub server: ServerConfig,
    pub database: DatabaseConfig,
    pub redis: RedisConfig,
    pub embedding: EmbeddingConfig,
    pub llm: LlmConfig,
    pub processor: ProcessorConfig,
    pub promotion: PromotionConfig,
    pub decay: DecayConfig,
    pub rate_limit: RateLimitConfig,
    pub retrieval: RetrievalConfig,
    pub telemetry: TelemetryConfig,
}

impl AppConfig {
    pub fn from_path(path: impl AsRef<Path>) -> Result<Self, ConfigError> {
        let raw = fs::read_to_string(path)?;
        Self::from_toml_str(&raw)
    }

    pub fn from_toml_str(raw: &str) -> Result<Self, ConfigError> {
        let config: Self = toml::from_str(raw)?;
        config.validate()?;
        Ok(config)
    }

    pub fn validate(&self) -> Result<(), ConfigError> {
        self.retrieval.weights.validate()?;
        validate_ratio(
            "promotion.promotion_threshold",
            self.promotion.promotion_threshold,
        )?;
        validate_ratio("decay.semantic.decay_rate", self.decay.semantic.decay_rate)?;
        validate_ratio(
            "decay.semantic.prune_threshold",
            self.decay.semantic.prune_threshold,
        )?;
        validate_ratio("decay.episodic.decay_rate", self.decay.episodic.decay_rate)?;
        validate_ratio(
            "decay.episodic.prune_threshold",
            self.decay.episodic.prune_threshold,
        )?;
        Ok(())
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
    pub request_timeout_secs: u64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DatabaseConfig {
    pub max_connections: u32,
    pub min_connections: u32,
    pub connect_timeout_secs: u64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RedisConfig {
    pub pool_size: u32,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EmbeddingConfig {
    pub provider: EmbeddingProviderKind,
    pub model: String,
    pub openai: Option<OpenAiEmbeddingConfig>,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum EmbeddingProviderKind {
    #[serde(rename = "fastembed")]
    FastEmbed,
    Openai,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OpenAiEmbeddingConfig {
    pub model: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LlmConfig {
    pub provider: LlmProviderKind,
    pub model: String,
    pub base_url: String,
    pub timeout_secs: u64,
    /// Optional Ollama-specific config (e.g. for Ollama Cloud which requires an API key).
    pub ollama: Option<OllamaConfig>,
    pub openai: Option<OpenAiLlmConfig>,
    pub anthropic: Option<AnthropicConfig>,
    pub openai_compatible: Option<OpenAiCompatibleConfig>,
}

/// Configuration block for Ollama-specific options.
///
/// The `api_key_env` field names an environment variable that holds the Bearer
/// token required by hosted / cloud Ollama deployments.  Local Ollama instances
/// do not need this — simply omit the `[llm.ollama]` section.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OllamaConfig {
    /// Name of the environment variable that holds the Ollama API key.
    /// Example: `"OLLAMA_API_KEY"` → the provider reads `std::env::var("OLLAMA_API_KEY")`.
    pub api_key_env: Option<String>,
}

impl OllamaConfig {
    /// Resolve the API key by reading the named environment variable.
    /// Returns `None` when `api_key_env` is not set or the variable is absent/empty.
    pub fn resolve_api_key(&self) -> Option<String> {
        let env_name = self.api_key_env.as_deref()?;
        match std::env::var(env_name) {
            Ok(value) if !value.trim().is_empty() => Some(value),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LlmProviderKind {
    Ollama,
    Openai,
    Anthropic,
    OpenaiCompatible,
    Openrouter,
    Huggingface,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OpenAiLlmConfig {
    pub model: String,
}

/// Configuration block for OpenAI-compatible providers (OpenRouter, Hugging Face, custom endpoints).
///
/// `api_key_env` names an environment variable that holds the Bearer token.
/// `headers` allows injecting arbitrary HTTP headers required by some providers
/// (e.g. `HTTP-Referer` for OpenRouter's usage tracking).
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OpenAiCompatibleConfig {
    /// Name of the environment variable that holds the API key.
    /// Example: `"OPENROUTER_API_KEY"` → reads `std::env::var("OPENROUTER_API_KEY")`.
    pub api_key_env: Option<String>,
    /// Extra HTTP headers to attach to every request (e.g. `HTTP-Referer`).
    #[serde(default)]
    pub headers: std::collections::BTreeMap<String, String>,
}

impl OpenAiCompatibleConfig {
    /// Resolve the API key by reading the named environment variable.
    /// Returns `None` when `api_key_env` is not set or the variable is absent/empty.
    pub fn resolve_api_key(&self) -> Option<String> {
        let env_name = self.api_key_env.as_deref()?;
        match std::env::var(env_name) {
            Ok(value) if !value.trim().is_empty() => Some(value),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AnthropicConfig {
    pub model: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProcessorConfig {
    pub fast_path_concurrency: usize,
    pub slow_path_workers: usize,
    pub max_retries: u32,
    pub dlq_ttl_days: u64,
    #[serde(default = "default_processing_stale_threshold_secs")]
    pub processing_stale_threshold_secs: u64,
    #[serde(default = "default_maintenance_window_hour_utc")]
    pub maintenance_window_hour_utc: u32,
    #[serde(default = "default_decay_window_hour_utc")]
    pub decay_window_hour_utc: u32,
}

fn default_processing_stale_threshold_secs() -> u64 {
    10 * 60
}

fn default_maintenance_window_hour_utc() -> u32 {
    2
}

fn default_decay_window_hour_utc() -> u32 {
    3
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PromotionConfig {
    pub cadence_minutes: u64,
    pub clustering_window_hours: u64,
    pub promotion_threshold: f32,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DecayConfig {
    pub schedule_hour_utc: u8,
    pub batch_size: u32,
    pub semantic: DecayClassConfig,
    pub episodic: DecayClassConfig,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DecayClassConfig {
    pub decay_rate: f32,
    pub prune_threshold: f32,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RateLimitConfig {
    pub retrieve_rpm: u32,
    pub ingest_rpm: u32,
    pub api_rpm: u32,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RetrievalConfig {
    pub dedup_threshold: f32,
    pub default_token_budget: usize,
    pub tokenizer: String,
    pub weights: ScoringWeights,
    pub source_authority: SourceAuthorityConfig,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScoringWeights {
    pub semantic_similarity: f32,
    pub importance: f32,
    pub recency: f32,
    pub source_authority: f32,
    pub memory_type: f32,
}

impl ScoringWeights {
    pub fn validate(&self) -> Result<(), ConfigError> {
        let sum = self.semantic_similarity
            + self.importance
            + self.recency
            + self.source_authority
            + self.memory_type;
        if (sum - 1.0).abs() > 1e-4 {
            return Err(ConfigError::WeightsMustSumToOne { sum });
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceAuthorityConfig {
    pub github: f32,
    pub slack: f32,
    pub jira: f32,
    pub linear: f32,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TelemetryConfig {
    pub log_format: LogFormat,
    pub otel_exporter: OtelExporter,
    pub slow_span_threshold_ms: u64,
    pub trace_retention_days: u64,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LogFormat {
    Json,
    Pretty,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OtelExporter {
    None,
    Otlp,
    Prometheus,
}

fn validate_ratio(field: &'static str, value: f32) -> Result<(), ConfigError> {
    if !(0.0..=1.0).contains(&value) {
        return Err(ConfigError::InvalidValue {
            field,
            message: format!("must be between 0.0 and 1.0, got {value}"),
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scoring_weights_must_sum_to_one() {
        let weights = ScoringWeights {
            semantic_similarity: 0.40,
            importance: 0.25,
            recency: 0.20,
            source_authority: 0.10,
            memory_type: 0.10,
        };

        assert!(weights.validate().is_err());
    }

    #[test]
    fn checked_in_config_deserializes() {
        let config = match AppConfig::from_toml_str(include_str!("../../../config.toml")) {
            Ok(config) => config,
            Err(error) => panic!("checked-in config.toml should deserialize: {error}"),
        };

        assert!(matches!(
            config.embedding.provider,
            EmbeddingProviderKind::FastEmbed
        ));
        assert!(matches!(config.llm.provider, LlmProviderKind::Ollama));
        assert_eq!(config.retrieval.default_token_budget, 4096);
    }

    #[test]
    fn ollama_config_resolves_api_key_from_env() {
        let cfg = OllamaConfig {
            api_key_env: Some("_TEST_OLLAMA_KEY_MEMORYOPS".to_owned()),
        };
        // Variable not set -> None
        std::env::remove_var("_TEST_OLLAMA_KEY_MEMORYOPS");
        assert!(cfg.resolve_api_key().is_none());

        // Variable set -> Some
        std::env::set_var("_TEST_OLLAMA_KEY_MEMORYOPS", "sk-test");
        assert_eq!(cfg.resolve_api_key().as_deref(), Some("sk-test"));
        std::env::remove_var("_TEST_OLLAMA_KEY_MEMORYOPS");
    }

    #[test]
    fn ollama_config_none_api_key_env_resolves_to_none() {
        let cfg = OllamaConfig { api_key_env: None };
        assert!(cfg.resolve_api_key().is_none());
    }

    #[test]
    fn openai_compatible_config_resolves_api_key_from_env() {
        let cfg = OpenAiCompatibleConfig {
            api_key_env: Some("_TEST_COMPAT_KEY_MEMORYOPS".to_owned()),
            headers: Default::default(),
        };
        // Variable not set -> None
        std::env::remove_var("_TEST_COMPAT_KEY_MEMORYOPS");
        assert!(cfg.resolve_api_key().is_none());

        // Variable set -> Some
        std::env::set_var("_TEST_COMPAT_KEY_MEMORYOPS", "sk-router-test");
        assert_eq!(cfg.resolve_api_key().as_deref(), Some("sk-router-test"));
        std::env::remove_var("_TEST_COMPAT_KEY_MEMORYOPS");
    }

    #[test]
    fn openai_compatible_config_none_api_key_env_resolves_to_none() {
        let cfg = OpenAiCompatibleConfig {
            api_key_env: None,
            headers: Default::default(),
        };
        assert!(cfg.resolve_api_key().is_none());
    }
}
