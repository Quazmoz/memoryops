use std::{collections::HashMap, env, sync::Arc};

use async_trait::async_trait;
use chrono::{Duration as ChronoDuration, Utc};
use common::{
    config::AppConfig,
    error::ProviderError,
    models::MemoryType,
    providers::{EmbeddingProvider, LlmProvider},
    AppState,
};
use processor::{
    embedder::{Embedder, COLLECTION_NAME},
    promoter::{run_promotion_pass, PromoterConfig},
    scheduler::run_decay_pass,
    store::{self, NewMemoryUnit},
    worker::{enqueue_slow_job, process_slow, ProcessorJob},
};
use qdrant_client::{
    qdrant::{
        point_id::PointIdOptions, Condition, Filter, PointStruct, ScoredPoint, SearchPointsBuilder,
        UpsertPointsBuilder,
    },
    Qdrant,
};
use serde_json::{json, Value};
use sqlx::PgPool;
use tokio::sync::Semaphore;
use uuid::Uuid;

const TEST_VECTOR: [f32; 3] = [0.1, 0.2, 0.3];

struct TestEmbeddingProvider;

#[async_trait]
impl EmbeddingProvider for TestEmbeddingProvider {
    async fn embed(&self, _text: &str) -> Result<Vec<f32>, ProviderError> {
        Ok(TEST_VECTOR.to_vec())
    }

    async fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>, ProviderError> {
        Ok(texts.iter().map(|_| TEST_VECTOR.to_vec()).collect())
    }

    fn dimensions(&self) -> usize {
        TEST_VECTOR.len()
    }

    fn model_name(&self) -> &str {
        "test-embedding"
    }
}

struct TestLlmProvider;

#[async_trait]
impl LlmProvider for TestLlmProvider {
    async fn complete(&self, prompt: &str) -> Result<String, ProviderError> {
        Ok(prompt.to_owned())
    }

    async fn summarize(&self, text: &str, _max_tokens: usize) -> Result<String, ProviderError> {
        Ok(text.to_owned())
    }
}

#[sqlx::test(migrations = "../../migrations")]
#[ignore = "requires docker-compose.test.yml services"]
async fn full_slow_path_integration(pool: PgPool) {
    let state = test_state(pool.clone()).await;
    ensure_collection(&state).await;
    let workspace_id = insert_workspace(&pool).await;
    let memory_id = insert_memory(&pool, workspace_id, "slow path memory", 1.0, None).await;
    let mut redis = state
        .redis
        .get()
        .await
        .unwrap_or_else(|error| panic!("test Redis should connect: {error}"));
    if let Err(error) = enqueue_slow_job(&mut *redis, memory_id, workspace_id, 0).await {
        panic!("slow job should enqueue: {error}");
    }

    process_memory(&state, memory_id, workspace_id).await;

    let embedding_id = memory_embedding_id(&pool, memory_id).await;
    let expected_embedding_id = memory_id.to_string();
    assert_eq!(
        embedding_id.as_deref(),
        Some(expected_embedding_id.as_str())
    );
    assert!(qdrant_contains_point(&state.qdrant, workspace_id, memory_id).await);
}

#[sqlx::test(migrations = "../../migrations")]
#[ignore = "requires docker-compose.test.yml services and MEMORYOPS_API_URL"]
async fn hybrid_search_with_vectors(pool: PgPool) {
    let Some(api_url) = api_base_url() else {
        eprintln!("MEMORYOPS_API_URL not set; skipping API-backed hybrid search assertion");
        return;
    };
    let state = test_state(pool.clone()).await;
    ensure_collection(&state).await;
    let client = reqwest::Client::new();
    let (workspace_id, api_key) = bootstrap_api_workspace(&client, &api_url).await;
    let memory_id = insert_memory(&pool, workspace_id, "hybrid vector memory", 1.0, None).await;
    process_memory(&state, memory_id, workspace_id).await;

    let response = api_post(
        &client,
        &api_url,
        "/v1/memory/search",
        Some(&api_key),
        json!({
            "query": "hybrid vector memory",
            "workspace_id": workspace_id,
            "mode": "hybrid",
            "limit": 10
        }),
    )
    .await;
    let results = response
        .get("results")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let memory_id_text = memory_id.to_string();
    let found = results.iter().any(|result| {
        result
            .get("memory")
            .and_then(|memory| memory.get("id"))
            .and_then(Value::as_str)
            == Some(memory_id_text.as_str())
            && result
                .get("memory")
                .and_then(|memory| memory.get("embedding_id"))
                .and_then(Value::as_str)
                .is_some()
    });

    assert!(found);
}

#[sqlx::test(migrations = "../../migrations")]
#[ignore = "requires docker-compose.test.yml services and MEMORYOPS_API_URL"]
async fn export_jsonl_cursor_paginates(pool: PgPool) {
    let Some(api_url) = api_base_url() else {
        eprintln!("MEMORYOPS_API_URL not set; skipping API-backed export assertion");
        return;
    };
    let client = reqwest::Client::new();
    let (workspace_id, api_key) = bootstrap_api_workspace(&client, &api_url).await;
    for index in 0..600 {
        let content = format!("export memory {index}");
        let _memory_id = insert_memory(&pool, workspace_id, &content, 0.8, None).await;
    }

    let response = match client
        .get(format!("{api_url}/v1/workspaces/{workspace_id}/export"))
        .header("x-api-key", api_key)
        .send()
        .await
    {
        Ok(response) => response,
        Err(error) => panic!("export request should respond: {error}"),
    };
    assert!(response.status().is_success());
    let body = match response.text().await {
        Ok(body) => body,
        Err(error) => panic!("export body should be readable: {error}"),
    };
    let lines = body.lines().collect::<Vec<_>>();

    assert_eq!(lines.len(), 600);
    assert!(lines.iter().all(|line| !line.contains("payload")));
    assert!(lines.iter().all(|line| !line.contains("embedding")));
}

#[sqlx::test(migrations = "../../migrations")]
#[ignore = "requires docker-compose.test.yml services"]
async fn scheduler_decay_pass_updates_scores(pool: PgPool) {
    let state = test_state(pool.clone()).await;
    let workspace_id = insert_workspace(&pool).await;
    let memory_id = insert_memory_with_created_at(
        &pool,
        workspace_id,
        "old memory",
        1.0,
        Utc::now() - ChronoDuration::days(60),
    )
    .await;

    if let Err(error) = run_decay_pass(&state).await {
        panic!("decay pass should complete: {error}");
    }

    let decay_score = memory_decay_score(&pool, memory_id).await;
    assert!(decay_score < 0.30);
}

#[sqlx::test(migrations = "../../migrations")]
#[ignore = "requires docker-compose.test.yml services"]
async fn promotion_pass_creates_semantic_unit(pool: PgPool) {
    let state = test_state(pool.clone()).await;
    ensure_collection(&state).await;
    let workspace_id = insert_workspace(&pool).await;
    let mut source_ids = Vec::new();

    for index in 0..4 {
        let memory_id = insert_memory_with_embedding(
            &pool,
            workspace_id,
            &format!("promotion memory {index}"),
            0.8,
            0.8,
        )
        .await;
        upsert_memory_vector(&state.qdrant, workspace_id, memory_id, [1.0, 0.0, 0.0]).await;
        source_ids.push(memory_id);
    }

    let report = match run_promotion_pass(
        &pool,
        &state.qdrant,
        state.llm_provider.as_ref(),
        state.embedding_provider.as_ref(),
        workspace_id,
        PromoterConfig {
            promotion_threshold: 0.72,
            dedup_cosine_threshold: 0.92,
            cluster_min_size: 3,
            batch_size: 200,
        },
    )
    .await
    {
        Ok(report) => report,
        Err(error) => panic!("promotion pass should complete: {error}"),
    };

    assert_eq!(report.clusters_found, 1);
    assert_eq!(report.units_promoted, 1);
    assert_eq!(semantic_count(&pool, workspace_id).await, 1);
    assert_eq!(deleted_source_count(&pool, &source_ids).await, 4);

    let (semantic_id, embedding_id, corroboration_count) = semantic_unit(&pool, workspace_id).await;
    assert!(embedding_id.is_some());
    assert_eq!(corroboration_count, 4);
    assert!(qdrant_contains_point(&state.qdrant, workspace_id, semantic_id).await);
}

#[sqlx::test(migrations = "../../migrations")]
#[ignore = "requires docker-compose.test.yml services"]
async fn dedup_cosine_threshold_prevents_false_clusters(pool: PgPool) {
    let state = test_state(pool.clone()).await;
    ensure_collection(&state).await;
    let workspace_id = insert_workspace(&pool).await;
    let vectors = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];

    for (index, vector) in vectors.iter().enumerate() {
        let memory_id = insert_memory_with_embedding(
            &pool,
            workspace_id,
            &format!("non cluster memory {index}"),
            0.8,
            0.8,
        )
        .await;
        upsert_memory_vector(&state.qdrant, workspace_id, memory_id, *vector).await;
    }

    let report = match run_promotion_pass(
        &pool,
        &state.qdrant,
        state.llm_provider.as_ref(),
        state.embedding_provider.as_ref(),
        workspace_id,
        PromoterConfig {
            promotion_threshold: 0.72,
            dedup_cosine_threshold: 0.92,
            cluster_min_size: 3,
            batch_size: 200,
        },
    )
    .await
    {
        Ok(report) => report,
        Err(error) => panic!("promotion pass should complete: {error}"),
    };

    assert_eq!(report.clusters_found, 0);
    assert_eq!(report.units_promoted, 0);
    assert_eq!(report.units_skipped, 3);
    assert_eq!(semantic_count(&pool, workspace_id).await, 0);
}

#[sqlx::test(migrations = "../../migrations")]
#[ignore = "requires docker-compose.test.yml services and MEMORYOPS_API_URL"]
async fn soft_delete_restore_cycle(pool: PgPool) {
    let Some(api_url) = api_base_url() else {
        eprintln!("MEMORYOPS_API_URL not set; skipping API-backed lifecycle assertion");
        return;
    };
    let state = test_state(pool.clone()).await;
    ensure_collection(&state).await;
    let client = reqwest::Client::new();
    let (workspace_id, api_key) = bootstrap_api_workspace(&client, &api_url).await;
    let memory_id =
        insert_memory(&pool, workspace_id, "soft delete restore memory", 1.0, None).await;
    process_memory(&state, memory_id, workspace_id).await;
    assert!(qdrant_contains_point(&state.qdrant, workspace_id, memory_id).await);

    let delete_response = match client
        .delete(format!("{api_url}/v1/memory/{memory_id}"))
        .header("x-api-key", &api_key)
        .send()
        .await
    {
        Ok(response) => response,
        Err(error) => panic!("delete request should respond: {error}"),
    };
    assert!(delete_response.status().is_success());
    assert!(!qdrant_contains_point(&state.qdrant, workspace_id, memory_id).await);

    let restore_response = match client
        .post(format!("{api_url}/v1/memory/{memory_id}/restore"))
        .header("x-api-key", &api_key)
        .send()
        .await
    {
        Ok(response) => response,
        Err(error) => panic!("restore request should respond: {error}"),
    };
    assert!(restore_response.status().is_success());
    assert!(memory_deleted_at_is_null(&pool, memory_id).await);
    assert!(processor_stream_mentions_memory(&state.redis, memory_id).await);
}

async fn test_state(pool: PgPool) -> AppState {
    let redis_url = env::var("REDIS_URL").unwrap_or_else(|_| "redis://localhost:16379".to_owned());
    let redis = deadpool_redis::Config::from_url(&redis_url)
        .create_pool(Some(deadpool_redis::Runtime::Tokio1))
        .unwrap_or_else(|error| panic!("test Redis pool should be created: {error}"));
    let qdrant_url = env::var("QDRANT_URL").unwrap_or_else(|_| "http://localhost:16333".to_owned());
    let qdrant = match Qdrant::from_url(&qdrant_url).build() {
        Ok(client) => client,
        Err(error) => panic!("test Qdrant URL should be valid: {error}"),
    };
    let config = match AppConfig::from_toml_str(include_str!("../../../config.toml")) {
        Ok(config) => config,
        Err(error) => panic!("checked-in config should parse: {error}"),
    };

    AppState {
        db: pool,
        redis,
        qdrant,
        processor_semaphore: Arc::new(Semaphore::new(
            usize::try_from(config.database.max_connections).unwrap_or(10),
        )),
        embedding_provider: Arc::new(TestEmbeddingProvider),
        llm_provider: Arc::new(TestLlmProvider),
        config: Arc::new(config),
        github_webhook_secret: "test-secret".to_owned(),
    }
}

async fn ensure_collection(state: &AppState) {
    let embedder = Embedder::from_state(state);
    if let Err(error) = embedder.ensure_collection().await {
        panic!("Qdrant collection should be ensured: {error}");
    }
}

async fn insert_workspace(pool: &PgPool) -> Uuid {
    let workspace_id = Uuid::now_v7();
    let result = sqlx::query("INSERT INTO workspaces (id, name, config) VALUES ($1, $2, $3)")
        .bind(workspace_id)
        .bind(format!("workspace-{workspace_id}"))
        .bind(json!({}))
        .execute(pool)
        .await;

    if let Err(error) = result {
        panic!("workspace insert should succeed: {error}");
    }

    workspace_id
}

async fn insert_memory(
    pool: &PgPool,
    workspace_id: Uuid,
    content: &str,
    importance_score: f32,
    embedding_id: Option<String>,
) -> Uuid {
    let unit = NewMemoryUnit {
        id: Uuid::now_v7(),
        workspace_id,
        scope: json!({
            "workspace_id": workspace_id,
            "agent_id": null,
            "user_id": null,
            "repo": "Quazmoz/memoryops"
        }),
        memory_type: MemoryType::Episodic,
        content: content.to_owned(),
        entities: json!([]),
        importance_score,
        source_events: Vec::new(),
        embedding_id,
        token_count: None,
        tags: Vec::new(),
    };
    match store::insert_memory_unit(pool, &unit).await {
        Ok(memory) => memory.id,
        Err(error) => panic!("memory insert should succeed: {error}"),
    }
}

async fn insert_memory_with_created_at(
    pool: &PgPool,
    workspace_id: Uuid,
    content: &str,
    importance_score: f32,
    created_at: chrono::DateTime<Utc>,
) -> Uuid {
    let memory_id = Uuid::now_v7();
    let result = sqlx::query(
        r#"
        INSERT INTO memory_units (
            id,
            workspace_id,
            scope,
            memory_type,
            content,
            entities,
            importance_score,
            tags,
            created_at,
            updated_at
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $9)
        "#,
    )
    .bind(memory_id)
    .bind(workspace_id)
    .bind(json!({
        "workspace_id": workspace_id,
        "agent_id": null,
        "user_id": null,
        "repo": "Quazmoz/memoryops"
    }))
    .bind(MemoryType::Episodic)
    .bind(content)
    .bind(json!([]))
    .bind(importance_score)
    .bind(Vec::<String>::new())
    .bind(created_at)
    .execute(pool)
    .await;

    if let Err(error) = result {
        panic!("memory insert should succeed: {error}");
    }

    memory_id
}

async fn insert_memory_with_embedding(
    pool: &PgPool,
    workspace_id: Uuid,
    content: &str,
    importance_score: f32,
    decay_score: f32,
) -> Uuid {
    let memory_id = Uuid::now_v7();
    let result = sqlx::query(
        r#"
        INSERT INTO memory_units (
            id,
            workspace_id,
            scope,
            memory_type,
            content,
            entities,
            importance_score,
            decay_score,
            embedding_id,
            tags
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
        "#,
    )
    .bind(memory_id)
    .bind(workspace_id)
    .bind(json!({
        "workspace_id": workspace_id,
        "agent_id": null,
        "user_id": null,
        "repo": "Quazmoz/memoryops"
    }))
    .bind(MemoryType::Episodic)
    .bind(content)
    .bind(json!([]))
    .bind(importance_score)
    .bind(decay_score)
    .bind(memory_id.to_string())
    .bind(Vec::<String>::new())
    .execute(pool)
    .await;

    if let Err(error) = result {
        panic!("memory with embedding insert should succeed: {error}");
    }

    memory_id
}

async fn upsert_memory_vector(
    qdrant: &Qdrant,
    workspace_id: Uuid,
    memory_id: Uuid,
    vector: [f32; 3],
) {
    let payload = HashMap::from([
        ("workspace_id".to_owned(), json!(workspace_id.to_string())),
        ("memory_type".to_owned(), json!("episodic")),
    ]);
    let point = PointStruct::new(memory_id.to_string(), vector.to_vec(), payload);
    let result = qdrant
        .upsert_points(UpsertPointsBuilder::new(COLLECTION_NAME, vec![point]).wait(true))
        .await;

    if let Err(error) = result {
        panic!("test vector upsert should succeed: {error}");
    }
}

async fn process_memory(state: &AppState, memory_id: Uuid, workspace_id: Uuid) {
    let job = ProcessorJob {
        stream_id: "integration-test".to_owned(),
        memory_id,
        workspace_id,
        attempts: 0,
    };
    if let Err(error) = process_slow(state, job).await {
        panic!("slow path should process memory: {error}");
    }
}

async fn memory_embedding_id(pool: &PgPool, memory_id: Uuid) -> Option<String> {
    match sqlx::query_scalar::<_, Option<String>>(
        "SELECT embedding_id FROM memory_units WHERE id = $1",
    )
    .bind(memory_id)
    .fetch_one(pool)
    .await
    {
        Ok(embedding_id) => embedding_id,
        Err(error) => panic!("embedding_id should be queryable: {error}"),
    }
}

async fn memory_decay_score(pool: &PgPool, memory_id: Uuid) -> f32 {
    match sqlx::query_scalar::<_, f32>("SELECT decay_score FROM memory_units WHERE id = $1")
        .bind(memory_id)
        .fetch_one(pool)
        .await
    {
        Ok(decay_score) => decay_score,
        Err(error) => panic!("decay_score should be queryable: {error}"),
    }
}

async fn memory_deleted_at_is_null(pool: &PgPool, memory_id: Uuid) -> bool {
    match sqlx::query_scalar::<_, bool>("SELECT deleted_at IS NULL FROM memory_units WHERE id = $1")
        .bind(memory_id)
        .fetch_one(pool)
        .await
    {
        Ok(is_null) => is_null,
        Err(error) => panic!("deleted_at should be queryable: {error}"),
    }
}

async fn semantic_count(pool: &PgPool, workspace_id: Uuid) -> i64 {
    match sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM memory_units WHERE workspace_id = $1 AND memory_type = 'semantic' AND deleted_at IS NULL",
    )
    .bind(workspace_id)
    .fetch_one(pool)
    .await
    {
        Ok(count) => count,
        Err(error) => panic!("semantic count should be queryable: {error}"),
    }
}

async fn deleted_source_count(pool: &PgPool, source_ids: &[Uuid]) -> i64 {
    match sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM memory_units WHERE id = ANY($1) AND deleted_at IS NOT NULL",
    )
    .bind(source_ids.to_vec())
    .fetch_one(pool)
    .await
    {
        Ok(count) => count,
        Err(error) => panic!("deleted source count should be queryable: {error}"),
    }
}

async fn semantic_unit(pool: &PgPool, workspace_id: Uuid) -> (Uuid, Option<String>, i32) {
    match sqlx::query_as::<_, (Uuid, Option<String>, i32)>(
        r#"
        SELECT id, embedding_id, corroboration_count
        FROM memory_units
        WHERE workspace_id = $1
          AND memory_type = 'semantic'
          AND deleted_at IS NULL
        LIMIT 1
        "#,
    )
    .bind(workspace_id)
    .fetch_one(pool)
    .await
    {
        Ok(row) => row,
        Err(error) => panic!("semantic unit should be queryable: {error}"),
    }
}

async fn qdrant_contains_point(qdrant: &Qdrant, workspace_id: Uuid, memory_id: Uuid) -> bool {
    let request =
        SearchPointsBuilder::new(COLLECTION_NAME, TEST_VECTOR.to_vec(), 10).filter(Filter::must(
            vec![Condition::matches("workspace_id", workspace_id.to_string())],
        ));
    let response = match qdrant.search_points(request).await {
        Ok(response) => response,
        Err(error) => panic!("Qdrant search should complete: {error}"),
    };

    response
        .result
        .iter()
        .any(|point| scored_point_has_uuid(point, memory_id))
}

fn scored_point_has_uuid(point: &ScoredPoint, memory_id: Uuid) -> bool {
    match point
        .id
        .as_ref()
        .and_then(|id| id.point_id_options.as_ref())
    {
        Some(PointIdOptions::Uuid(value)) => value == &memory_id.to_string(),
        Some(PointIdOptions::Num(_)) | None => false,
    }
}

fn api_base_url() -> Option<String> {
    env::var("MEMORYOPS_API_URL")
        .ok()
        .map(|url| url.trim_end_matches('/').to_owned())
}

async fn bootstrap_api_workspace(client: &reqwest::Client, api_url: &str) -> (Uuid, String) {
    let response = api_post(
        client,
        api_url,
        "/v1/workspaces",
        None,
        json!({ "name": format!("workspace-{}", Uuid::now_v7()) }),
    )
    .await;
    let workspace_id = parse_workspace_id(&response);
    let key_response = api_post(
        client,
        api_url,
        &format!("/v1/workspaces/{workspace_id}/keys"),
        None,
        json!({ "name": "integration-test" }),
    )
    .await;
    let api_key = key_response
        .get("plaintext_key")
        .or_else(|| key_response.get("key"))
        .and_then(Value::as_str)
        .map(str::to_owned);

    match api_key {
        Some(api_key) => (workspace_id, api_key),
        None => panic!("key response should include plaintext key"),
    }
}

async fn api_post(
    client: &reqwest::Client,
    api_url: &str,
    path: &str,
    api_key: Option<&str>,
    body: Value,
) -> Value {
    let mut request = client.post(format!("{api_url}{path}")).json(&body);
    if let Some(api_key) = api_key {
        request = request.header("x-api-key", api_key);
    }
    let response = match request.send().await {
        Ok(response) => response,
        Err(error) => panic!("API request should respond: {error}"),
    };
    let status = response.status();
    let payload = match response.json::<Value>().await {
        Ok(payload) => payload,
        Err(error) => panic!("API response should be JSON: {error}"),
    };
    if !status.is_success() {
        panic!("API request failed with {status}: {payload}");
    }
    payload
}

fn parse_workspace_id(response: &Value) -> Uuid {
    let raw = response
        .get("id")
        .or_else(|| response.get("workspace_id"))
        .and_then(Value::as_str);
    match raw.and_then(|value| Uuid::parse_str(value).ok()) {
        Some(workspace_id) => workspace_id,
        None => panic!("workspace response should include a UUID id"),
    }
}

async fn processor_stream_mentions_memory(redis: &deadpool_redis::Pool, memory_id: Uuid) -> bool {
    let mut connection = match redis.get().await {
        Ok(conn) => conn,
        Err(error) => panic!("test Redis should connect: {error}"),
    };
    let value = match redis::cmd("XRANGE")
        .arg("processor_jobs")
        .arg("-")
        .arg("+")
        .query_async::<redis::Value>(&mut *connection)
        .await
    {
        Ok(value) => value,
        Err(error) => panic!("processor_jobs stream should be queryable: {error}"),
    };

    format!("{value:?}").contains(&memory_id.to_string())
}
