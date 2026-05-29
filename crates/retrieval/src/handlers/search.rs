use std::collections::HashMap;

use anyhow::anyhow;
use async_trait::async_trait;
use axum::{extract::Query, extract::State, Extension, Json};
use common::{
    auth::AuthContext, build_embedding_provider_for_workspace, error::AppResult,
    models::MemoryUnit, AppError, AppState,
};
use qdrant_client::{
    qdrant::{point_id::PointIdOptions, Condition, Filter, ScoredPoint, SearchPointsBuilder},
    Qdrant,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use validator::Validate;

use crate::{
    access,
    dto::{SearchMode, SearchRequest, SearchResponse, DEFAULT_LIMIT, MAX_LIMIT},
    promotion,
    search::{hybrid, keyword, vector},
    store,
};

use super::resolve_workspace_id;

const MEMORY_SEARCH_DEFAULT_LIMIT: u32 = 10;
const MEMORY_SEARCH_MAX_LIMIT: u32 = 50;

#[derive(Debug, Clone, Deserialize)]
pub struct MemorySearchQuery {
    pub q: Option<String>,
    pub limit: Option<u32>,
    pub repo: Option<String>,
    pub agent_id: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MemorySearchResponse {
    pub memories: Vec<MemoryUnit>,
    pub total: u64,
}

#[derive(Debug, Clone)]
struct MemorySearchOptions {
    workspace_id: Uuid,
    query: String,
    limit: u32,
    repo: Option<String>,
    agent_id: Option<String>,
}

#[derive(Debug, Clone)]
struct VectorLookupOptions {
    embedding: Vec<f32>,
    workspace_id: Uuid,
    repo: Option<String>,
    agent_id: Option<String>,
    limit: u32,
}

#[async_trait]
trait MemorySearchEmbedding: Send + Sync {
    async fn embed_query(&self, query: &str) -> Result<Vec<f32>, common::ProviderError>;
}

#[async_trait]
impl MemorySearchEmbedding for std::sync::Arc<dyn common::providers::EmbeddingProvider> {
    async fn embed_query(&self, query: &str) -> Result<Vec<f32>, common::ProviderError> {
        self.embed(query).await
    }
}

#[async_trait]
trait MemorySearchQdrant: Send + Sync {
    async fn search_memory_ids(&self, options: VectorLookupOptions) -> AppResult<Vec<Uuid>>;
}

#[async_trait]
impl MemorySearchQdrant for Qdrant {
    async fn search_memory_ids(&self, options: VectorLookupOptions) -> AppResult<Vec<Uuid>> {
        let request = SearchPointsBuilder::new(
            crate::search::vector::COLLECTION_NAME,
            options.embedding,
            u64::from(options.limit),
        )
        .filter(build_workspace_filter(
            options.workspace_id,
            options.repo,
            options.agent_id,
        ));

        let response = self
            .search_points(request)
            .await
            .map_err(|error| AppError::Internal(anyhow!(error)))?;

        Ok(response
            .result
            .into_iter()
            .filter_map(scored_point_uuid)
            .collect())
    }
}

#[async_trait]
trait MemorySearchStore: Send + Sync {
    async fn get_memory_units_by_ids(
        &self,
        ids: &[Uuid],
        workspace_id: Uuid,
    ) -> AppResult<Vec<MemoryUnit>>;
}

struct PgMemorySearchStore<'a> {
    db: &'a sqlx::PgPool,
}

#[async_trait]
impl MemorySearchStore for PgMemorySearchStore<'_> {
    async fn get_memory_units_by_ids(
        &self,
        ids: &[Uuid],
        workspace_id: Uuid,
    ) -> AppResult<Vec<MemoryUnit>> {
        store::get_memory_units_by_ids(self.db, ids, workspace_id).await
    }
}

#[axum::debug_handler]
pub async fn handle_memory_search(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthContext>,
    Query(params): Query<MemorySearchQuery>,
) -> AppResult<Json<MemorySearchResponse>> {
    let options = MemorySearchOptions {
        workspace_id: auth.workspace_id,
        query: required_query(params.q.as_deref())?,
        limit: resolve_memory_search_limit(params.limit),
        repo: params.repo,
        agent_id: params.agent_id,
    };

    let memory_store = PgMemorySearchStore { db: &state.db };
    let config = super::fetch_workspace_config(&state, options.workspace_id).await?;
    let embedding_provider = build_embedding_provider_for_workspace(&state.config, &config);
    let response =
        run_memory_search(&embedding_provider, &state.qdrant, &memory_store, options).await?;

    Ok(Json(response))
}

fn resolve_memory_search_limit(value: Option<u32>) -> u32 {
    value
        .unwrap_or(MEMORY_SEARCH_DEFAULT_LIMIT)
        .min(MEMORY_SEARCH_MAX_LIMIT)
}

fn required_query(value: Option<&str>) -> AppResult<String> {
    let Some(query) = value.map(str::trim).filter(|query| !query.is_empty()) else {
        return Err(AppError::Validation("missing q query parameter".to_owned()));
    };

    Ok(query.to_owned())
}

fn build_workspace_filter(
    workspace_id: Uuid,
    repo: Option<String>,
    agent_id: Option<String>,
) -> Filter {
    let mut conditions = vec![Condition::matches("workspace_id", workspace_id.to_string())];

    if let Some(repo) = repo {
        conditions.push(Condition::matches("repo", repo));
    }

    if let Some(agent_id) = agent_id {
        conditions.push(Condition::matches("agent_id", agent_id));
    }

    Filter::must(conditions)
}

fn scored_point_uuid(point: ScoredPoint) -> Option<Uuid> {
    match point.id?.point_id_options? {
        PointIdOptions::Uuid(value) => Uuid::parse_str(&value).ok(),
        PointIdOptions::Num(_) => None,
    }
}

async fn run_memory_search(
    embedding_provider: &dyn MemorySearchEmbedding,
    qdrant: &dyn MemorySearchQdrant,
    memory_store: &dyn MemorySearchStore,
    options: MemorySearchOptions,
) -> AppResult<MemorySearchResponse> {
    let embedding = embedding_provider
        .embed_query(&options.query)
        .await
        .map_err(AppError::Provider)?;

    let ids = qdrant
        .search_memory_ids(VectorLookupOptions {
            embedding,
            workspace_id: options.workspace_id,
            repo: options.repo,
            agent_id: options.agent_id,
            limit: options.limit,
        })
        .await?;

    let units = memory_store
        .get_memory_units_by_ids(&ids, options.workspace_id)
        .await?;
    let mut units_by_id = units
        .into_iter()
        .map(|unit| (unit.id, unit))
        .collect::<HashMap<_, _>>();
    let memories = ids
        .into_iter()
        .filter_map(|id| units_by_id.remove(&id))
        .collect::<Vec<_>>();

    Ok(MemorySearchResponse {
        total: memories.len() as u64,
        memories,
    })
}

#[axum::debug_handler]
pub async fn handle_search(
    State(state): State<AppState>,
    auth: Option<Extension<AuthContext>>,
    Json(mut req): Json<SearchRequest>,
) -> AppResult<Json<SearchResponse>> {
    Validate::validate(&req).map_err(|error| AppError::Validation(error.to_string()))?;

    let limit = req.limit.unwrap_or(DEFAULT_LIMIT).min(MAX_LIMIT);
    let auth_context = auth.as_ref().map(|extension| &extension.0);
    let workspace_id = resolve_workspace_id(auth_context, Some(req.workspace_id))?;
    req.workspace_id = workspace_id;
    let config = super::fetch_workspace_config(&state, workspace_id).await?;
    req.apply_workspace_config(&config);
    let results = match req.mode {
        SearchMode::Vector => vector::vector_search_results(&state, &req, limit).await?,
        SearchMode::Keyword => keyword::keyword_search(&state, &req, limit).await?,
        SearchMode::Hybrid => hybrid::hybrid_search(&state, &req, limit).await?,
    };

    // Batch record access for all memory IDs
    let memory_ids: Vec<uuid::Uuid> = results.iter().map(|result| result.memory.id).collect();
    if let Err(error) = access::record_access_batch(&state.redis, &memory_ids).await {
        tracing::warn!(error = ?error, count = memory_ids.len(), "failed to batch record memory access");
    }

    let result_ids = results
        .iter()
        .map(|result| result.memory.id)
        .collect::<Vec<_>>();
    if !result_ids.is_empty() {
        let task_state = state.clone();
        let config = config.clone();
        tokio::spawn(async move {
            promotion::check_and_promote(task_state, workspace_id, result_ids, &config).await;
        });
    }

    Ok(Json(SearchResponse {
        total: results.len() as u64,
        results,
        query_id: Uuid::now_v7(),
    }))
}

#[cfg(test)]
mod tests {
    use std::{
        sync::{
            atomic::{AtomicUsize, Ordering},
            Mutex,
        },
        vec,
    };

    use chrono::Utc;
    use common::{
        models::{MemoryScope, MemoryType, ScopeVisibility},
        ProviderError,
    };
    use sqlx::types::Json as SqlxJson;

    use serde_json::json;

    use super::*;

    #[test]
    fn search_request_validation_rejects_empty_query() {
        let workspace_id = Uuid::now_v7();
        let request = match serde_json::from_value::<SearchRequest>(json!({
            "query": "",
            "workspace_id": workspace_id,
            "mode": "hybrid"
        })) {
            Ok(request) => request,
            Err(error) => panic!("request should deserialize before validation: {error}"),
        };

        assert!(Validate::validate(&request).is_err());
    }

    #[test]
    fn search_request_validation_rejects_overlimit() {
        let workspace_id = Uuid::now_v7();
        let request = match serde_json::from_value::<SearchRequest>(json!({
            "query": "memory",
            "workspace_id": workspace_id,
            "mode": "keyword",
            "limit": 101
        })) {
            Ok(request) => request,
            Err(error) => panic!("request should deserialize before validation: {error}"),
        };

        assert!(Validate::validate(&request).is_err());
    }

    #[derive(Default)]
    struct MockEmbedding {
        calls: AtomicUsize,
    }

    #[async_trait]
    impl MemorySearchEmbedding for MockEmbedding {
        async fn embed_query(&self, _query: &str) -> Result<Vec<f32>, common::ProviderError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            Ok(vec![0.2, 0.4, 0.6])
        }
    }

    #[derive(Debug, Clone)]
    struct CapturedQdrantRequest {
        workspace_id: Uuid,
        repo: Option<String>,
        agent_id: Option<String>,
        limit: u32,
        embedding_len: usize,
    }

    struct MockQdrant {
        ids: Vec<Uuid>,
        captured: Mutex<Option<CapturedQdrantRequest>>,
    }

    #[async_trait]
    impl MemorySearchQdrant for MockQdrant {
        async fn search_memory_ids(&self, options: VectorLookupOptions) -> AppResult<Vec<Uuid>> {
            let captured = CapturedQdrantRequest {
                workspace_id: options.workspace_id,
                repo: options.repo,
                agent_id: options.agent_id,
                limit: options.limit,
                embedding_len: options.embedding.len(),
            };

            match self.captured.lock() {
                Ok(mut slot) => *slot = Some(captured),
                Err(error) => return Err(AppError::Internal(anyhow!(error.to_string()))),
            }

            Ok(self.ids.clone())
        }
    }

    struct MockStore {
        items: Vec<MemoryUnit>,
        captured_ids: Mutex<Vec<Uuid>>,
    }

    #[async_trait]
    impl MemorySearchStore for MockStore {
        async fn get_memory_units_by_ids(
            &self,
            ids: &[Uuid],
            _workspace_id: Uuid,
        ) -> AppResult<Vec<MemoryUnit>> {
            match self.captured_ids.lock() {
                Ok(mut captured) => *captured = ids.to_vec(),
                Err(error) => return Err(AppError::Internal(anyhow!(error.to_string()))),
            }
            Ok(self.items.clone())
        }
    }

    fn test_memory_unit(id: Uuid, workspace_id: Uuid, label: &str) -> MemoryUnit {
        let now = Utc::now();
        MemoryUnit {
            id,
            workspace_id,
            scope: MemoryScope {
                workspace_id,
                source: None,
                actor: None,
                agent_id: Some("agent-7".to_owned()),
                user_id: None,
                repo: Some("Quazmoz/memoryops".to_owned()),
            },
            memory_type: MemoryType::Semantic,
            scope_visibility: ScopeVisibility::Private,
            content: label.to_owned(),
            entities: SqlxJson(Vec::new()),
            importance_score: 0.8,
            importance_overridden: false,
            source_events: Vec::new(),
            embedding_id: None,
            token_count: Some(4),
            decay_score: 1.0,
            relevance_score: 0.5,
            pinned: false,
            tags: Vec::new(),
            version: 1,
            promoted_at: None,
            source_episode_ids: Vec::new(),
            corroboration_count: 0,
            deleted_at: None,
            last_accessed_at: None,
            created_at: now,
            updated_at: now,
        }
    }

    #[test]
    fn memory_search_limit_defaults_and_caps() {
        assert_eq!(resolve_memory_search_limit(None), 10);
        assert_eq!(resolve_memory_search_limit(Some(3)), 3);
        assert_eq!(resolve_memory_search_limit(Some(999)), 50);
    }

    #[test]
    fn required_query_rejects_missing_or_blank_values() {
        assert!(required_query(None).is_err());
        assert!(required_query(Some("   ")).is_err());

        let query = match required_query(Some("  find this  ")) {
            Ok(query) => query,
            Err(error) => panic!("non-empty query should be accepted: {error}"),
        };
        assert_eq!(query, "find this");
    }

    #[tokio::test]
    async fn run_memory_search_uses_workspace_scoped_qdrant_lookup() {
        let workspace_id = Uuid::now_v7();
        let first_id = Uuid::now_v7();
        let second_id = Uuid::now_v7();

        let embedding = MockEmbedding::default();
        let qdrant = MockQdrant {
            ids: vec![second_id, first_id],
            captured: Mutex::new(None),
        };
        let store = MockStore {
            items: vec![
                test_memory_unit(first_id, workspace_id, "first"),
                test_memory_unit(second_id, workspace_id, "second"),
            ],
            captured_ids: Mutex::new(Vec::new()),
        };

        let response = match run_memory_search(
            &embedding,
            &qdrant,
            &store,
            MemorySearchOptions {
                workspace_id,
                query: "search text".to_owned(),
                limit: 25,
                repo: Some("Quazmoz/memoryops".to_owned()),
                agent_id: Some("agent-7".to_owned()),
            },
        )
        .await
        {
            Ok(response) => response,
            Err(error) => panic!("run_memory_search should succeed: {error}"),
        };

        assert_eq!(embedding.calls.load(Ordering::SeqCst), 1);
        assert_eq!(response.total, 2);
        assert_eq!(response.memories.len(), 2);
        assert_eq!(response.memories[0].id, second_id);
        assert_eq!(response.memories[1].id, first_id);

        let captured = match qdrant.captured.lock() {
            Ok(guard) => guard.clone(),
            Err(error) => panic!("captured qdrant state should lock: {error}"),
        };
        let captured = match captured {
            Some(captured) => captured,
            None => panic!("qdrant should capture request details"),
        };

        assert_eq!(captured.workspace_id, workspace_id);
        assert_eq!(captured.repo.as_deref(), Some("Quazmoz/memoryops"));
        assert_eq!(captured.agent_id.as_deref(), Some("agent-7"));
        assert_eq!(captured.limit, 25);
        assert_eq!(captured.embedding_len, 3);

        let captured_ids = match store.captured_ids.lock() {
            Ok(guard) => guard.clone(),
            Err(error) => panic!("captured ids should lock: {error}"),
        };
        assert_eq!(captured_ids, vec![second_id, first_id]);
    }

    #[derive(Default)]
    struct FailingEmbedding;

    #[async_trait]
    impl MemorySearchEmbedding for FailingEmbedding {
        async fn embed_query(&self, _query: &str) -> Result<Vec<f32>, common::ProviderError> {
            Err(ProviderError::Request("boom".to_owned()))
        }
    }

    #[tokio::test]
    async fn run_memory_search_surfaces_embedding_errors() {
        let workspace_id = Uuid::now_v7();
        let qdrant = MockQdrant {
            ids: Vec::new(),
            captured: Mutex::new(None),
        };
        let store = MockStore {
            items: Vec::new(),
            captured_ids: Mutex::new(Vec::new()),
        };

        let error = match run_memory_search(
            &FailingEmbedding,
            &qdrant,
            &store,
            MemorySearchOptions {
                workspace_id,
                query: "search text".to_owned(),
                limit: 10,
                repo: None,
                agent_id: None,
            },
        )
        .await
        {
            Ok(_) => panic!("embedding failure should return an error"),
            Err(error) => error,
        };

        assert!(matches!(error, AppError::Provider(_)));
    }
}
