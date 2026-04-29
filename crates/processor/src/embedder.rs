use std::{collections::HashMap, sync::Arc, time::Instant};

use anyhow::anyhow;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use common::{
    error::AppResult,
    models::{MemoryType, ScopeVisibility},
    providers::EmbeddingProvider,
    telemetry::EMBEDDING_LATENCY,
    AppError, AppState,
};
use qdrant_client::{
    qdrant::{
        CreateCollectionBuilder, DeletePointsBuilder, Distance, PointStruct, UpsertPointsBuilder,
        VectorParamsBuilder,
    },
    Qdrant,
};
use serde_json::json;
use uuid::Uuid;

pub type QdrantClient = Qdrant;

pub const COLLECTION_NAME: &str = "memoryops_memories";

#[derive(Debug, Clone, PartialEq)]
pub struct QdrantPayload {
    pub workspace_id: Uuid,
    pub memory_type: MemoryType,
    pub scope_visibility: ScopeVisibility,
    pub importance_score: f32,
    pub decay_score: f32,
    pub created_at: DateTime<Utc>,
    pub agent_id: Option<String>,
    pub user_id: Option<String>,
    pub repo: Option<String>,
    pub tags: Vec<String>,
}

impl QdrantPayload {
    pub fn from_memory_unit(memory: &common::models::MemoryUnit) -> Self {
        Self {
            workspace_id: memory.workspace_id,
            memory_type: memory.memory_type,
            scope_visibility: memory.scope_visibility,
            importance_score: memory.importance_score,
            decay_score: memory.decay_score,
            created_at: memory.created_at,
            agent_id: memory.scope.agent_id.clone(),
            user_id: memory.scope.user_id.clone(),
            repo: memory.scope.repo.clone(),
            tags: memory.tags.clone(),
        }
    }

    pub(crate) fn into_qdrant_payload(self) -> HashMap<String, serde_json::Value> {
        HashMap::from([
            (
                "workspace_id".to_owned(),
                json!(self.workspace_id.to_string()),
            ),
            (
                "memory_type".to_owned(),
                json!(memory_type_as_str(self.memory_type)),
            ),
            (
                "scope_visibility".to_owned(),
                json!(scope_visibility_as_str(self.scope_visibility)),
            ),
            ("importance_score".to_owned(), json!(self.importance_score)),
            ("decay_score".to_owned(), json!(self.decay_score)),
            ("created_at".to_owned(), json!(self.created_at.to_rfc3339())),
            ("agent_id".to_owned(), json!(self.agent_id)),
            ("user_id".to_owned(), json!(self.user_id)),
            ("repo".to_owned(), json!(self.repo)),
            ("tags".to_owned(), json!(self.tags)),
        ])
    }
}

#[derive(Clone)]
pub struct Embedder {
    provider: Arc<dyn EmbeddingProvider>,
    qdrant: QdrantClient,
}

impl Embedder {
    pub fn new(provider: Arc<dyn EmbeddingProvider>, qdrant: QdrantClient) -> Self {
        Self { provider, qdrant }
    }

    pub fn from_state(state: &AppState) -> Self {
        Self::new(state.embedding_provider.clone(), state.qdrant.clone())
    }

    pub async fn ensure_collection(&self) -> AppResult<()> {
        let dimensions = self.provider.dimensions();
        if dimensions == 0 {
            return Err(AppError::Provider(common::ProviderError::NotConfigured));
        }

        let exists = self
            .qdrant
            .collection_exists(COLLECTION_NAME)
            .await
            .map_err(|error| AppError::Internal(anyhow!(error)))?;
        if exists {
            return Ok(());
        }

        self.qdrant
            .create_collection(
                CreateCollectionBuilder::new(COLLECTION_NAME)
                    .vectors_config(VectorParamsBuilder::new(
                        dimensions as u64,
                        Distance::Cosine,
                    ))
                    .on_disk_payload(true),
            )
            .await
            .map(|_| ())
            .map_err(|error| AppError::Internal(anyhow!(error)))
    }

    pub async fn embed_and_store(
        &self,
        memory_id: Uuid,
        _workspace_id: Uuid,
        text: &str,
        payload: QdrantPayload,
    ) -> AppResult<String> {
        embed_and_store_with_writer(
            self.provider.as_ref(),
            &self.qdrant,
            memory_id,
            text,
            payload,
        )
        .await
    }

    pub async fn delete_point(&self, memory_id: Uuid) -> AppResult<()> {
        delete_point_with_writer(&self.qdrant, memory_id).await
    }
}

async fn embed_and_store_with_writer(
    embedding_provider: &dyn EmbeddingProvider,
    writer: &dyn QdrantPointWriter,
    memory_id: Uuid,
    text: &str,
    payload: QdrantPayload,
) -> AppResult<String> {
    let started = Instant::now();
    let vector = embedding_provider.embed(text).await?;
    EMBEDDING_LATENCY.record(started.elapsed().as_secs_f64() * 1_000.0, &[]);
    let point_id = memory_id.to_string();
    let point = PointStruct::new(point_id.clone(), vector, payload.into_qdrant_payload());
    writer.upsert_point(point).await?;
    Ok(point_id)
}

async fn delete_point_with_writer(
    writer: &dyn QdrantPointWriter,
    memory_id: Uuid,
) -> AppResult<()> {
    writer.delete_point_id(memory_id.to_string()).await
}

#[async_trait]
trait QdrantPointWriter: Send + Sync {
    async fn upsert_point(&self, point: PointStruct) -> AppResult<()>;

    async fn delete_point_id(&self, point_id: String) -> AppResult<()>;
}

#[async_trait]
impl QdrantPointWriter for Qdrant {
    async fn upsert_point(&self, point: PointStruct) -> AppResult<()> {
        self.upsert_points(UpsertPointsBuilder::new(COLLECTION_NAME, vec![point]).wait(true))
            .await
            .map(|_| ())
            .map_err(|error| AppError::Internal(anyhow!(error)))
    }

    async fn delete_point_id(&self, point_id: String) -> AppResult<()> {
        self.delete_points(
            DeletePointsBuilder::new(COLLECTION_NAME)
                .points([point_id])
                .wait(true),
        )
        .await
        .map(|_| ())
        .map_err(|error| AppError::Internal(anyhow!(error)))
    }
}

fn memory_type_as_str(memory_type: MemoryType) -> &'static str {
    match memory_type {
        MemoryType::Episodic => "episodic",
        MemoryType::Semantic => "semantic",
    }
}

fn scope_visibility_as_str(scope_visibility: ScopeVisibility) -> &'static str {
    match scope_visibility {
        ScopeVisibility::Private => "private",
        ScopeVisibility::Workspace => "workspace",
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use async_trait::async_trait;
    use common::error::ProviderError;
    use qdrant_client::qdrant::PointStruct;

    use super::*;

    struct MockEmbeddingProvider;

    #[async_trait]
    impl EmbeddingProvider for MockEmbeddingProvider {
        async fn embed(&self, _text: &str) -> Result<Vec<f32>, ProviderError> {
            Ok(vec![0.1, 0.2, 0.3])
        }

        async fn embed_batch(&self, _texts: &[&str]) -> Result<Vec<Vec<f32>>, ProviderError> {
            Ok(vec![vec![0.1, 0.2, 0.3]])
        }

        fn dimensions(&self) -> usize {
            3
        }

        fn model_name(&self) -> &str {
            "mock"
        }
    }

    #[derive(Default)]
    struct MockQdrantWriter {
        upserted: Mutex<Vec<PointStruct>>,
        deleted: Mutex<Vec<String>>,
    }

    #[async_trait]
    impl QdrantPointWriter for MockQdrantWriter {
        async fn upsert_point(&self, point: PointStruct) -> AppResult<()> {
            match self.upserted.lock() {
                Ok(mut upserted) => upserted.push(point),
                Err(error) => return Err(AppError::Internal(anyhow!(error.to_string()))),
            }
            Ok(())
        }

        async fn delete_point_id(&self, point_id: String) -> AppResult<()> {
            match self.deleted.lock() {
                Ok(mut deleted) => deleted.push(point_id),
                Err(error) => return Err(AppError::Internal(anyhow!(error.to_string()))),
            }
            Ok(())
        }
    }

    #[tokio::test]
    async fn embed_and_store_returns_memory_id_as_point_id() {
        let provider = MockEmbeddingProvider;
        let writer = MockQdrantWriter::default();
        let memory_id = Uuid::now_v7();
        let payload = QdrantPayload {
            workspace_id: Uuid::now_v7(),
            memory_type: MemoryType::Episodic,
            scope_visibility: ScopeVisibility::Private,
            importance_score: 0.8,
            decay_score: 0.7,
            created_at: Utc::now(),
            agent_id: None,
            user_id: None,
            repo: None,
            tags: Vec::new(),
        };

        let point_id = match embed_and_store_with_writer(
            &provider,
            &writer,
            memory_id,
            "memory text",
            payload,
        )
        .await
        {
            Ok(point_id) => point_id,
            Err(error) => panic!("embedder should upsert: {error}"),
        };

        assert_eq!(point_id, memory_id.to_string());
    }

    #[tokio::test]
    async fn delete_point_calls_qdrant_delete() {
        let writer = MockQdrantWriter::default();
        let memory_id = Uuid::now_v7();

        if let Err(error) = delete_point_with_writer(&writer, memory_id).await {
            panic!("delete point should call writer: {error}");
        }

        let deleted = match writer.deleted.lock() {
            Ok(deleted) => deleted,
            Err(error) => panic!("mock writer mutex should not be poisoned: {error}"),
        };
        assert_eq!(deleted.as_slice(), &[memory_id.to_string()]);
    }

    #[tokio::test]
    async fn embed_and_store_upserts_point_with_payload() {
        let provider = MockEmbeddingProvider;
        let writer = MockQdrantWriter::default();
        let memory_id = Uuid::now_v7();
        let workspace_id = Uuid::now_v7();
        let payload = QdrantPayload {
            workspace_id,
            memory_type: MemoryType::Episodic,
            scope_visibility: ScopeVisibility::Private,
            importance_score: 0.8,
            decay_score: 0.7,
            created_at: Utc::now(),
            agent_id: Some("agent".to_owned()),
            user_id: None,
            repo: Some("Quazmoz/memoryops".to_owned()),
            tags: vec!["rust".to_owned()],
        };

        let point_id = match embed_and_store_with_writer(
            &provider,
            &writer,
            memory_id,
            "memory text",
            payload,
        )
        .await
        {
            Ok(point_id) => point_id,
            Err(error) => panic!("embedder should upsert: {error}"),
        };

        let upserted = match writer.upserted.lock() {
            Ok(upserted) => upserted,
            Err(error) => panic!("mock writer mutex should not be poisoned: {error}"),
        };
        assert_eq!(point_id, memory_id.to_string());
        assert_eq!(upserted.len(), 1);
        assert!(upserted[0].payload.contains_key("workspace_id"));
        assert!(upserted[0].payload.contains_key("scope_visibility"));
        assert!(upserted[0].payload.contains_key("created_at"));
    }
}
