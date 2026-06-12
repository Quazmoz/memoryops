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
        self.database.validate()?;
        self.processor.validate()?;
        self.rate_limit.validate()?;
        self.retrieval.validate()?;
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
    #[serde(default)]
    pub allow_private_ips: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DatabaseConfig {
    pub max_connections: u32,
    pub min_connections: u32,
    pub connect_timeout_secs: u64,
}

impl DatabaseConfig {
    fn validate(&self) -> Result<(), ConfigError> {
        if self.max_connections == 0 {
            return Err(ConfigError::InvalidValue {
                field: "database.max_connections",
                message: "must be at least 1".to_owned(),
            });
        }
        if self.min_connections > self.max_connections {
            return Err(ConfigError::InvalidValue {
                field: "database.min_connections",
                message: format!(
                    "must be less than or equal to database.max_connections ({})",
                    self.max_connections
                ),
            });
        }
        if self.connect_timeout_secs == 0 {
            return Err(ConfigError::InvalidValue {
                field: "database.connect_timeout_secs",
                message: "must be at least 1".to_owned(),
            });
        }
        Ok(())
    }
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
    #[serde(default)]
    pub base_url: Option<String>,
    pub timeout_secs: u64,
    /// Optional Ollama-specific config (e.g. for Ollama Cloud which requires an API key).
    pub ollama: Option<OllamaConfig>,
    pub openai: Option<OpenAiLlmConfig>,
    pub anthropic: Option<AnthropicConfig>,
    pub openai_compatible: Option<OpenAiCompatibleConfig>,
    pub gemini: Option<GeminiConfig>,
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
    Gemini,
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

/// Configuration block for Google Gemini.
///
/// Uses the Gemini REST API (`generativelanguage.googleapis.com`), not the
/// OpenAI-compatible shim.  `api_key_env` names the environment variable that
/// holds a Google AI Studio or Vertex AI API key.
///
/// Example config.toml:
/// ```toml
/// [llm]
/// provider = "gemini"
/// model    = "gemini-2.0-flash"
///
/// [llm.gemini]
/// api_key_env = "GEMINI_API_KEY"
/// ```
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeminiConfig {
    /// Name of the environment variable that holds the Gemini API key.
    /// Example: `"GEMINI_API_KEY"` → reads `std::env::var("GEMINI_API_KEY")`.
    pub api_key_env: Option<String>,
}

impl GeminiConfig {
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

impl ProcessorConfig {
    fn validate(&self) -> Result<(), ConfigError> {
        if self.fast_path_concurrency == 0 {
            return Err(ConfigError::InvalidValue {
                field: "processor.fast_path_concurrency",
                message: "must be at least 1".to_owned(),
            });
        }
        if self.slow_path_workers == 0 {
            return Err(ConfigError::InvalidValue {
                field: "processor.slow_path_workers",
                message: "must be at least 1".to_owned(),
            });
        }
        if self.dlq_ttl_days == 0 {
            return Err(ConfigError::InvalidValue {
                field: "processor.dlq_ttl_days",
                message: "must be at least 1".to_owned(),
            });
        }
        if self.processing_stale_threshold_secs == 0 {
            return Err(ConfigError::InvalidValue {
                field: "processor.processing_stale_threshold_secs",
                message: "must be at least 1".to_owned(),
            });
        }
        validate_utc_hour(
            "processor.maintenance_window_hour_utc",
            self.maintenance_window_hour_utc,
        )?;
        validate_utc_hour(
            "processor.decay_window_hour_utc",
            self.decay_window_hour_utc,
        )?;
        Ok(())
    }
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
    /// Separate bucket for high-frequency dashboard polling routes so they
    /// cannot exhaust the workspace's general API quota.
    pub dashboard_rpm: u32,
}

impl RateLimitConfig {
    fn validate(&self) -> Result<(), ConfigError> {
        validate_positive_u32("rate_limit.retrieve_rpm", self.retrieve_rpm)?;
        validate_positive_u32("rate_limit.ingest_rpm", self.ingest_rpm)?;
        validate_positive_u32("rate_limit.api_rpm", self.api_rpm)?;
        validate_positive_u32("rate_limit.dashboard_rpm", self.dashboard_rpm)?;
        Ok(())
    }
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

impl RetrievalConfig {
    fn validate(&self) -> Result<(), ConfigError> {
        validate_ratio("retrieval.dedup_threshold", self.dedup_threshold)?;
        if self.default_token_budget == 0 {
            return Err(ConfigError::InvalidValue {
                field: "retrieval.default_token_budget",
                message: "must be at least 1".to_owned(),
            });
        }
        if self.tokenizer.trim().is_empty() {
            return Err(ConfigError::InvalidValue {
                field: "retrieval.tokenizer",
                message: "must not be empty".to_owned(),
            });
        }
        self.weights.validate()?;
        self.source_authority.validate()?;
        Ok(())
    }
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
        validate_ratio(
            "retrieval.weights.semantic_similarity",
            self.semantic_similarity,
        )?;
        validate_ratio("retrieval.weights.importance", self.importance)?;
        validate_ratio("retrieval.weights.recency", self.recency)?;
        validate_ratio("retrieval.weights.source_authority", self.source_authority)?;
        validate_ratio("retrieval.weights.memory_type", self.memory_type)?;

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

impl SourceAuthorityConfig {
    fn validate(&self) -> Result<(), ConfigError> {
        validate_ratio("retrieval.source_authority.github", self.github)?;
        validate_ratio("retrieval.source_authority.slack", self.slack)?;
        validate_ratio("retrieval.source_authority.jira", self.jira)?;
        validate_ratio("retrieval.source_authority.linear", self.linear)?;
        Ok(())
    }
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

fn validate_positive_u32(field: &'static str, value: u32) -> Result<(), ConfigError> {
    if value == 0 {
        return Err(ConfigError::InvalidValue {
            field,
            message: "must be at least 1".to_owned(),
        });
    }
    Ok(())
}

fn validate_utc_hour(field: &'static str, value: u32) -> Result<(), ConfigError> {
    if value > 23 {
        return Err(ConfigError::InvalidValue {
            field,
            message: format!("must be a UTC hour between 0 and 23, got {value}"),
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scoring_weights_not_summing_to_one_fails_validation() {
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
    fn scoring_weights_summing_to_one_passes_validation() {
        let weights = ScoringWeights {
            semantic_similarity: 0.40,
            importance: 0.25,
            recency: 0.20,
            source_authority: 0.10,
            memory_type: 0.05,
        };

        assert!(weights.validate().is_ok());
    }

    #[test]
    fn scoring_weights_outside_ratio_range_fail_validation() {
        let weights = ScoringWeights {
            semantic_similarity: 1.20,
            importance: -0.20,
            recency: 0.0,
            source_authority: 0.0,
            memory_type: 0.0,
        };

        assert!(weights.validate().is_err());
    }

    #[test]
    fn invalid_processor_window_hour_fails_validation() {
        let config = ProcessorConfig {
            fast_path_concurrency: 1,
            slow_path_workers: 1,
            max_retries: 3,
            dlq_ttl_days: 7,
            processing_stale_threshold_secs: 600,
            maintenance_window_hour_utc: 24,
            decay_window_hour_utc: 3,
        };

        assert!(config.validate().is_err());
    }

    #[test]
    fn invalid_database_pool_bounds_fail_validation() {
        let config = DatabaseConfig {
            max_connections: 4,
            min_connections: 5,
            connect_timeout_secs: 5,
        };

        assert!(config.validate().is_err());
    }

    #[test]
    fn zero_rate_limit_fails_validation() {
        let config = RateLimitConfig {
            retrieve_rpm: 60,
            ingest_rpm: 300,
            api_rpm: 0,
            dashboard_rpm: 600,
        };

        assert!(config.validate().is_err());
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
        std::env::remove_var("_TEST_OLLAMA_KEY_MEMORYOPS");
        assert!(cfg.resolve_api_key().is_none());

        std::env::set_var("_TEST_OLLAMA_KEY_MEMORYOPS", "sk-test");
        assert_eq!(cfg.resolve_api_key().as_deref(), Some("sk-test"));
        std::env::remove_var("_TEST_OLLAMA_KEY_MEMORYOPS");
    }

    #[test]
    fn ollama_config_none_api_key_env_resolves_to_none() {
        let cfg = OllamaConfig { api_key_env: None };
        assert!(cfg.resolve_api_key().is_none());
    }
}
