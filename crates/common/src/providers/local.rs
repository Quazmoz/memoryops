use async_trait::async_trait;

use crate::{error::ProviderError, providers::EmbeddingProvider};

const DEFAULT_FASTEMBED_DIMENSIONS: usize = 384;

pub struct FastEmbedProvider {
    model: String,
    dimensions: usize,
}

impl FastEmbedProvider {
    pub fn new(model: impl Into<String>) -> Self {
        Self {
            model: model.into(),
            dimensions: DEFAULT_FASTEMBED_DIMENSIONS,
        }
    }
}

#[async_trait]
impl EmbeddingProvider for FastEmbedProvider {
    async fn embed(&self, text: &str) -> Result<Vec<f32>, ProviderError> {
        Ok(hash_embedding(text, self.dimensions))
    }

    async fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>, ProviderError> {
        Ok(texts
            .iter()
            .map(|text| hash_embedding(text, self.dimensions))
            .collect())
    }

    fn dimensions(&self) -> usize {
        self.dimensions
    }

    fn model_name(&self) -> &str {
        &self.model
    }
}

fn hash_embedding(text: &str, dimensions: usize) -> Vec<f32> {
    if dimensions == 0 {
        return Vec::new();
    }

    let mut vector = vec![0.0_f32; dimensions];
    for token in text.split_whitespace() {
        let hash = fnv1a(token.as_bytes());
        let index = (hash as usize) % dimensions;
        let sign = if hash & 1 == 0 { 1.0 } else { -1.0 };
        vector[index] += sign;
    }

    let norm = vector.iter().map(|value| value * value).sum::<f32>().sqrt();
    if norm > 0.0 {
        for value in &mut vector {
            *value /= norm;
        }
    }

    vector
}

fn fnv1a(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf29ce484222325_u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn local_embeddings_are_deterministic_and_normalized() {
        let provider = FastEmbedProvider::new("test-model");
        let left = provider
            .embed("memory ops memory")
            .await
            .unwrap_or_default();
        let right = provider
            .embed("memory ops memory")
            .await
            .unwrap_or_default();
        let norm = left.iter().map(|value| value * value).sum::<f32>().sqrt();

        assert_eq!(left, right);
        assert_eq!(left.len(), provider.dimensions());
        assert!((norm - 1.0).abs() < 0.0001);
    }
}
