use std::collections::HashMap;

use common::{
    error::{AppResult, ProviderError},
    AppError, AppState,
};
use qdrant_client::qdrant::{PointStruct, UpsertPointsBuilder};
use uuid::Uuid;

pub const COLLECTION_NAME: &str = "memory_units";

pub async fn embed_and_upsert(
    state: &AppState,
    memory_id: &Uuid,
    content: &str,
) -> AppResult<String> {
    embed_and_upsert_with_provider(
        state.embedding_provider.as_ref(),
        Some(&state.qdrant),
        memory_id,
        content,
    )
    .await
}

async fn embed_and_upsert_with_provider(
    embedding_provider: &dyn common::providers::EmbeddingProvider,
    qdrant: Option<&qdrant_client::Qdrant>,
    memory_id: &Uuid,
    content: &str,
) -> AppResult<String> {
    let embedding_id = memory_id.to_string();
    let embedding_vector = match embedding_provider.embed(content).await {
        Ok(embedding_vector) => embedding_vector,
        Err(ProviderError::NotConfigured) => {
            tracing::debug!(memory_id = %memory_id, "embedding provider not configured; skipping Qdrant upsert");
            return Ok(embedding_id);
        }
        Err(error) => return Err(AppError::Provider(error)),
    };

    if let Some(qdrant) = qdrant {
        let mut payload = HashMap::new();
        payload.insert("memory_id".to_owned(), serde_json::json!(embedding_id));
        payload.insert("memory_type".to_owned(), serde_json::json!("episodic"));

        let point = PointStruct::new(memory_id.to_string(), embedding_vector, payload);
        let request = UpsertPointsBuilder::new(COLLECTION_NAME, vec![point]);

        if let Err(error) = qdrant.upsert_points(request).await {
            tracing::warn!(error = ?error, memory_id = %memory_id, "Qdrant upsert failed; continuing without failing pipeline");
        }
    }

    Ok(memory_id.to_string())
}

#[cfg(test)]
mod tests {
    use async_trait::async_trait;
    use common::{error::ProviderError, providers::EmbeddingProvider};

    use super::*;

    struct NotConfiguredProvider;

    #[async_trait]
    impl EmbeddingProvider for NotConfiguredProvider {
        async fn embed(&self, _text: &str) -> Result<Vec<f32>, ProviderError> {
            Err(ProviderError::NotConfigured)
        }

        async fn embed_batch(&self, _texts: &[&str]) -> Result<Vec<Vec<f32>>, ProviderError> {
            Err(ProviderError::NotConfigured)
        }

        fn dimensions(&self) -> usize {
            0
        }

        fn model_name(&self) -> &str {
            "not-configured"
        }
    }

    #[tokio::test]
    async fn not_configured_provider_returns_id_without_panicking() {
        let provider = NotConfiguredProvider;
        let memory_id = Uuid::now_v7();

        let embedding_id =
            match embed_and_upsert_with_provider(&provider, None, &memory_id, "hello").await {
                Ok(embedding_id) => embedding_id,
                Err(error) => panic!("not configured providers should be non-fatal: {error}"),
            };

        assert_eq!(embedding_id, memory_id.to_string());
    }
}
