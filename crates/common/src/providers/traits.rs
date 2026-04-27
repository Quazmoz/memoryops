use async_trait::async_trait;

use crate::error::ProviderError;

#[async_trait]
pub trait EmbeddingProvider: Send + Sync + 'static {
    async fn embed(&self, text: &str) -> Result<Vec<f32>, ProviderError>;

    async fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>, ProviderError>;

    fn dimensions(&self) -> usize;

    fn model_name(&self) -> &str;
}

#[async_trait]
pub trait LlmProvider: Send + Sync + 'static {
    async fn complete(&self, prompt: &str) -> Result<String, ProviderError>;

    async fn summarize(&self, text: &str, max_tokens: usize) -> Result<String, ProviderError>;
}
