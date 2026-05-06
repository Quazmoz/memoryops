use std::time::Duration;

use async_trait::async_trait;
use reqwest::{header, Client, StatusCode};
use serde_json::{json, Value};

use crate::{
    error::ProviderError,
    providers::{EmbeddingProvider, LlmProvider},
};

fn client_with_timeout(timeout_secs: u64) -> Client {
    Client::builder()
        .timeout(Duration::from_secs(timeout_secs))
        .build()
        .unwrap_or_default()
}

pub struct OllamaProvider {
    client: Client,
    base_url: String,
    model: String,
    /// Bearer token for cloud / hosted Ollama deployments that require authentication.
    /// `None` for local instances where no auth is needed.
    api_key: Option<String>,
}

impl OllamaProvider {
    pub fn new(
        base_url: impl Into<String>,
        model: impl Into<String>,
        timeout_secs: u64,
        api_key: Option<String>,
    ) -> Self {
        Self {
            client: client_with_timeout(timeout_secs),
            base_url: base_url.into().trim_end_matches('/').to_owned(),
            model: model.into(),
            api_key,
        }
    }

    /// Attach an `Authorization: Bearer <token>` header when an API key is
    /// configured.  Returns the request builder unchanged for local instances.
    fn maybe_auth(&self, builder: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        match self.api_key.as_deref() {
            Some(key) => builder.bearer_auth(key),
            None => builder,
        }
    }
}

#[async_trait]
impl LlmProvider for OllamaProvider {
    async fn complete(&self, prompt: &str) -> Result<String, ProviderError> {
        let request = self
            .client
            .post(format!("{}/api/generate", self.base_url))
            .json(&json!({ "model": self.model, "prompt": prompt, "stream": false }));
        let response = self
            .maybe_auth(request)
            .send()
            .await
            .map_err(|error| ProviderError::Request(error.to_string()))?;
        let payload = response_json(response).await?;
        payload
            .get("response")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned)
            .ok_or_else(|| ProviderError::InvalidResponse("missing Ollama response".to_owned()))
    }

    async fn summarize(&self, text: &str, max_tokens: usize) -> Result<String, ProviderError> {
        let prompt = format!(
            "Summarize this MemoryOps memory in at most {max_tokens} tokens. Preserve concrete names, repositories, decisions, and outcomes.\n\n{text}"
        );
        let request = self
            .client
            .post(format!("{}/api/generate", self.base_url))
            .json(&json!({
                "model": self.model,
                "prompt": prompt,
                "stream": false,
                "options": { "num_predict": max_tokens }
            }));
        let response = self
            .maybe_auth(request)
            .send()
            .await
            .map_err(|error| ProviderError::Request(error.to_string()))?;
        let payload = response_json(response).await?;
        payload
            .get("response")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned)
            .ok_or_else(|| ProviderError::InvalidResponse("missing Ollama response".to_owned()))
    }
}

pub struct OpenAIEmbedProvider {
    client: Client,
    api_key: Option<String>,
    model: String,
}

impl OpenAIEmbedProvider {
    pub fn new(model: impl Into<String>, api_key: Option<String>, timeout_secs: u64) -> Self {
        Self {
            client: client_with_timeout(timeout_secs),
            api_key,
            model: model.into(),
        }
    }
}

#[async_trait]
impl EmbeddingProvider for OpenAIEmbedProvider {
    async fn embed(&self, text: &str) -> Result<Vec<f32>, ProviderError> {
        let Some(api_key) = self.api_key.as_ref() else {
            return Err(ProviderError::NotConfigured);
        };
        let response = self
            .client
            .post("https://api.openai.com/v1/embeddings")
            .bearer_auth(api_key)
            .json(&json!({ "model": self.model, "input": text }))
            .send()
            .await
            .map_err(|error| ProviderError::Request(error.to_string()))?;
        let payload = response_json(response).await?;
        parse_openai_embedding(&payload)
    }

    async fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>, ProviderError> {
        let Some(api_key) = self.api_key.as_ref() else {
            return Err(ProviderError::NotConfigured);
        };
        let response = self
            .client
            .post("https://api.openai.com/v1/embeddings")
            .bearer_auth(api_key)
            .json(&json!({ "model": self.model, "input": texts }))
            .send()
            .await
            .map_err(|error| ProviderError::Request(error.to_string()))?;
        let payload = response_json(response).await?;
        let data = payload
            .get("data")
            .and_then(Value::as_array)
            .ok_or_else(|| {
                ProviderError::InvalidResponse("missing OpenAI embedding data".to_owned())
            })?;

        data.iter().map(parse_embedding_item).collect()
    }

    fn dimensions(&self) -> usize {
        if self.model.contains("3-large") {
            3072
        } else {
            1536
        }
    }

    fn model_name(&self) -> &str {
        &self.model
    }
}

pub struct OpenAIProvider {
    client: Client,
    api_key: Option<String>,
    model: String,
}

impl OpenAIProvider {
    pub fn new(model: impl Into<String>, api_key: Option<String>, timeout_secs: u64) -> Self {
        Self {
            client: client_with_timeout(timeout_secs),
            api_key,
            model: model.into(),
        }
    }
}

#[async_trait]
impl LlmProvider for OpenAIProvider {
    async fn complete(&self, prompt: &str) -> Result<String, ProviderError> {
        self.chat(prompt, None).await
    }

    async fn summarize(&self, text: &str, max_tokens: usize) -> Result<String, ProviderError> {
        let prompt = format!(
            "Summarize this MemoryOps memory in at most {max_tokens} tokens. Preserve concrete names, repositories, decisions, and outcomes.\n\n{text}"
        );
        self.chat(&prompt, Some(max_tokens)).await
    }
}

impl OpenAIProvider {
    async fn chat(&self, prompt: &str, max_tokens: Option<usize>) -> Result<String, ProviderError> {
        let Some(api_key) = self.api_key.as_ref() else {
            return Err(ProviderError::NotConfigured);
        };
        let mut body = json!({
            "model": self.model,
            "messages": [{ "role": "user", "content": prompt }]
        });
        if let Some(max_tokens) = max_tokens {
            body["max_tokens"] = json!(max_tokens);
        }
        let response = self
            .client
            .post("https://api.openai.com/v1/chat/completions")
            .bearer_auth(api_key)
            .json(&body)
            .send()
            .await
            .map_err(|error| ProviderError::Request(error.to_string()))?;
        let payload = response_json(response).await?;
        payload
            .pointer("/choices/0/message/content")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned)
            .ok_or_else(|| {
                ProviderError::InvalidResponse("missing OpenAI message content".to_owned())
            })
    }
}

/// A generic provider for any OpenAI-compatible `/chat/completions` endpoint.
///
/// Covers:
/// - `provider = "openai_compatible"` (arbitrary self-hosted or third-party endpoint)
/// - `provider = "openrouter"` (routes to <https://openrouter.ai/api/v1>)
/// - `provider = "huggingface"` (routes to <https://router.huggingface.co/v1>)
pub struct OpenAiCompatibleProvider {
    client: Client,
    api_key: Option<String>,
    model: String,
    base_url: String,
    extra_headers: std::collections::BTreeMap<String, String>,
}

impl OpenAiCompatibleProvider {
    pub fn new(
        base_url: impl Into<String>,
        model: impl Into<String>,
        api_key: Option<String>,
        extra_headers: std::collections::BTreeMap<String, String>,
    ) -> Self {
        Self {
            client: Client::new(),
            api_key,
            model: model.into(),
            base_url: base_url.into().trim_end_matches('/').to_owned(),
            extra_headers,
        }
    }

    /// Build a POST request to `{base_url}/chat/completions`, optionally attaching
    /// a Bearer token and any provider-specific extra headers.
    fn build_request(&self, body: &serde_json::Value) -> reqwest::RequestBuilder {
        let mut builder = self
            .client
            .post(format!("{}/chat/completions", self.base_url))
            .json(body);
        if let Some(key) = self.api_key.as_deref() {
            builder = builder.bearer_auth(key);
        }
        for (name, value) in &self.extra_headers {
            builder = builder.header(name.as_str(), value.as_str());
        }
        builder
    }
}

#[async_trait]
impl LlmProvider for OpenAiCompatibleProvider {
    async fn complete(&self, prompt: &str) -> Result<String, ProviderError> {
        let body = json!({
            "model": self.model,
            "messages": [{ "role": "user", "content": prompt }]
        });
        let response = self
            .build_request(&body)
            .send()
            .await
            .map_err(|error| ProviderError::Request(error.to_string()))?;
        let payload = response_json(response).await?;
        payload
            .pointer("/choices/0/message/content")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned)
            .ok_or_else(|| {
                ProviderError::InvalidResponse(
                    "missing openai-compatible message content".to_owned(),
                )
            })
    }

    async fn summarize(&self, text: &str, max_tokens: usize) -> Result<String, ProviderError> {
        let prompt = format!(
            "Summarize this MemoryOps memory in at most {max_tokens} tokens. Preserve concrete names, repositories, decisions, and outcomes.\n\n{text}"
        );
        let mut body = json!({
            "model": self.model,
            "messages": [{ "role": "user", "content": prompt }]
        });
        body["max_tokens"] = json!(max_tokens);
        let response = self
            .build_request(&body)
            .send()
            .await
            .map_err(|error| ProviderError::Request(error.to_string()))?;
        let payload = response_json(response).await?;
        payload
            .pointer("/choices/0/message/content")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned)
            .ok_or_else(|| {
                ProviderError::InvalidResponse(
                    "missing openai-compatible message content".to_owned(),
                )
            })
    }
}

pub struct AnthropicProvider {
    client: Client,
    api_key: Option<String>,
    model: String,
}

impl AnthropicProvider {
    pub fn new(model: impl Into<String>, api_key: Option<String>, timeout_secs: u64) -> Self {
        Self {
            client: client_with_timeout(timeout_secs),
            api_key,
            model: model.into(),
        }
    }
}

#[async_trait]
impl LlmProvider for AnthropicProvider {
    async fn complete(&self, prompt: &str) -> Result<String, ProviderError> {
        self.message(prompt, None).await
    }

    async fn summarize(&self, text: &str, max_tokens: usize) -> Result<String, ProviderError> {
        let prompt = format!(
            "Summarize this MemoryOps memory in at most {max_tokens} tokens. Preserve concrete names, repositories, decisions, and outcomes.\n\n{text}"
        );
        self.message(&prompt, Some(max_tokens)).await
    }
}

impl AnthropicProvider {
    async fn message(
        &self,
        prompt: &str,
        max_tokens: Option<usize>,
    ) -> Result<String, ProviderError> {
        let Some(api_key) = self.api_key.as_ref() else {
            return Err(ProviderError::NotConfigured);
        };
        let response = self
            .client
            .post("https://api.anthropic.com/v1/messages")
            .header("x-api-key", api_key)
            .header("anthropic-version", "2023-06-01")
            .json(&json!({
                "model": self.model,
                "max_tokens": max_tokens.unwrap_or(1024),
                "messages": [{ "role": "user", "content": prompt }]
            }))
            .send()
            .await
            .map_err(|error| ProviderError::Request(error.to_string()))?;
        let payload = response_json(response).await?;
        payload
            .pointer("/content/0/text")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned)
            .ok_or_else(|| {
                ProviderError::InvalidResponse("missing Anthropic text content".to_owned())
            })
    }
}

/// Provider for Google Gemini via the native REST API.
///
/// Uses `POST https://generativelanguage.googleapis.com/v1beta/models/{model}:generateContent?key={api_key}`
/// — *not* the OpenAI-compatible shim — so it works correctly for local
/// development without any extra proxy setup.
///
/// Supported models: `gemini-2.0-flash`, `gemini-2.0-flash-lite`, `gemini-1.5-pro`, etc.
pub struct GeminiProvider {
    client: Client,
    api_key: Option<String>,
    model: String,
}

impl GeminiProvider {
    pub fn new(model: impl Into<String>, api_key: Option<String>, timeout_secs: u64) -> Self {
        Self {
            client: client_with_timeout(timeout_secs),
            api_key,
            model: model.into(),
        }
    }

    fn endpoint(&self) -> String {
        format!(
            "https://generativelanguage.googleapis.com/v1beta/models/{}:generateContent",
            self.model
        )
    }

    async fn generate(
        &self,
        prompt: &str,
        max_tokens: Option<usize>,
    ) -> Result<String, ProviderError> {
        let Some(api_key) = self.api_key.as_ref() else {
            return Err(ProviderError::NotConfigured);
        };

        let mut body = json!({
            "contents": [{
                "parts": [{ "text": prompt }]
            }]
        });

        if let Some(max_tokens) = max_tokens {
            body["generationConfig"] = json!({ "maxOutputTokens": max_tokens });
        }

        let response = self
            .client
            .post(self.endpoint())
            .query(&[("key", api_key.as_str())])
            .json(&body)
            .send()
            .await
            .map_err(|error| ProviderError::Request(error.to_string()))?;

        let payload = response_json(response).await?;

        payload
            .pointer("/candidates/0/content/parts/0/text")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned)
            .ok_or_else(|| {
                ProviderError::InvalidResponse("missing Gemini response text".to_owned())
            })
    }
}

#[async_trait]
impl LlmProvider for GeminiProvider {
    async fn complete(&self, prompt: &str) -> Result<String, ProviderError> {
        self.generate(prompt, None).await
    }

    async fn summarize(&self, text: &str, max_tokens: usize) -> Result<String, ProviderError> {
        let prompt = format!(
            "Summarize this MemoryOps memory in at most {max_tokens} tokens. Preserve concrete names, repositories, decisions, and outcomes.\n\n{text}"
        );
        self.generate(&prompt, Some(max_tokens)).await
    }
}

async fn response_json(response: reqwest::Response) -> Result<Value, ProviderError> {
    if response.status() == StatusCode::TOO_MANY_REQUESTS {
        let retry_after_secs = response
            .headers()
            .get(header::RETRY_AFTER)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.parse::<u64>().ok())
            .unwrap_or(60);
        return Err(ProviderError::RateLimited { retry_after_secs });
    }
    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(ProviderError::Request(format!(
            "provider returned HTTP {status}: {body}"
        )));
    }

    response
        .json::<Value>()
        .await
        .map_err(|error| ProviderError::InvalidResponse(error.to_string()))
}

fn parse_openai_embedding(payload: &Value) -> Result<Vec<f32>, ProviderError> {
    let item = payload.pointer("/data/0").ok_or_else(|| {
        ProviderError::InvalidResponse("missing OpenAI embedding item".to_owned())
    })?;
    parse_embedding_item(item)
}

fn parse_embedding_item(item: &Value) -> Result<Vec<f32>, ProviderError> {
    item.get("embedding")
        .and_then(Value::as_array)
        .ok_or_else(|| ProviderError::InvalidResponse("missing embedding vector".to_owned()))?
        .iter()
        .map(|value| {
            value.as_f64().map(|number| number as f32).ok_or_else(|| {
                ProviderError::InvalidResponse("embedding value was not numeric".to_owned())
            })
        })
        .collect()
}
