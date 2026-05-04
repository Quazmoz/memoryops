use std::{net::SocketAddr, sync::Arc, time::Duration};

use anyhow::anyhow;
use axum::{
    extract::State, http::StatusCode, middleware as axum_middleware, routing::get, Json, Router,
};
use chrono::Utc;
use common::{
    config::{AppConfig, EmbeddingProviderKind, LlmProviderKind},
    providers::{
        AnthropicProvider, EmbeddingProvider, FastEmbedProvider, LlmProvider, OllamaProvider,
        OpenAIEmbedProvider, OpenAIProvider,
    },
    telemetry::init_telemetry,
    AppState,
};
use qdrant_client::Qdrant;
use retrieval::retrieval_router;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::postgres::PgPoolOptions;
use tokio::sync::Semaphore;

mod handlers;
mod middleware;
mod security;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config_path = std::env::var("CONFIG_PATH").unwrap_or_else(|_| "config.toml".to_owned());
    let config = AppConfig::from_path(config_path)?;
    let _telemetry_guard = init_telemetry(&config.telemetry)?;
    crate::security::validate_secret_key_at_startup()
        .map_err(|_| anyhow!("APP_SECRET_KEY is missing or invalid -- cannot start"))?;
    let state = build_state(config.clone()).await?;
    processor::start_workers(state.clone()).await?;
    tokio::spawn(processor::scheduler::run_scheduler(state.clone()));

    let address = format!("{}:{}", config.server.host, config.server.port);
    let listener = tokio::net::TcpListener::bind(&address).await?;

    tracing::info!(%address, "starting MemoryOps API");
    axum::serve(
        listener,
        router(state).into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await?;

    Ok(())
}

fn router(state: AppState) -> Router {
    let ingestion_router = ingestion::ingestion_router().layer(
        axum_middleware::from_fn_with_state(state.clone(), middleware::rate_limit::rate_limit),
    );
    let observation_router = ingestion::observation_router()
        .layer(axum_middleware::from_fn_with_state(
            state.clone(),
            middleware::rate_limit::rate_limit,
        ))
        .layer(axum_middleware::from_fn_with_state(
            state.clone(),
            middleware::auth::require_api_key,
        ));
    let retrieval_router = retrieval_router()
        .layer(axum_middleware::from_fn_with_state(
            state.clone(),
            middleware::rate_limit::rate_limit,
        ))
        .layer(axum_middleware::from_fn_with_state(
            state.clone(),
            middleware::auth::require_api_key,
        ));
    let protected_api_router = handlers::protected_router()
        .layer(axum_middleware::from_fn_with_state(
            state.clone(),
            middleware::rate_limit::rate_limit,
        ))
        .layer(axum_middleware::from_fn_with_state(
            state.clone(),
            middleware::auth::require_api_key,
        ));

    Router::new()
        .route("/health", get(health))
        .route("/health/ready", get(readiness))
        .route("/health/system", get(system_health))
        .merge(handlers::bootstrap_router())
        .merge(ingestion_router)
        .merge(observation_router)
        .merge(retrieval_router)
        .merge(protected_api_router)
        .with_state(state)
}

async fn build_state(config: AppConfig) -> anyhow::Result<AppState> {
    let database_url =
        std::env::var("DATABASE_URL").map_err(|_| anyhow::anyhow!("DATABASE_URL not set"))?;
    let redis_url = std::env::var("REDIS_URL").map_err(|_| anyhow::anyhow!("REDIS_URL not set"))?;
    let qdrant_url =
        std::env::var("QDRANT_URL").map_err(|_| anyhow::anyhow!("QDRANT_URL not set"))?;
    let github_webhook_secret = webhook_secret_from_env("GITHUB_WEBHOOK_SECRET")?;

    let db = PgPoolOptions::new()
        .max_connections(config.database.max_connections)
        .min_connections(config.database.min_connections)
        .acquire_timeout(Duration::from_secs(config.database.connect_timeout_secs))
        .connect(&database_url)
        .await?;
    ensure_skill_secret_configuration(&db).await?;
    let redis = {
        let cfg = deadpool_redis::Config::from_url(&redis_url);
        cfg.create_pool(Some(deadpool_redis::Runtime::Tokio1))
            .expect("failed to create Redis pool")
    };
    let qdrant = Qdrant::from_url(&qdrant_url).build()?;

    let embedding_provider = build_embedding_provider(&config);
    let llm_provider = build_llm_provider(&config);

    Ok(AppState {
        db,
        redis,
        qdrant,
        processor_semaphore: Arc::new(Semaphore::new(
            usize::try_from(config.database.max_connections).unwrap_or(10),
        )),
        embedding_provider,
        llm_provider,
        config: Arc::new(config),
        github_webhook_secret,
    })
}

async fn ensure_skill_secret_configuration(db: &sqlx::PgPool) -> anyhow::Result<()> {
    let skills_table = sqlx::query_scalar::<_, Option<String>>(
        "SELECT to_regclass('public.workspace_skills')::TEXT",
    )
    .fetch_one(db)
    .await?;
    if skills_table.is_none() {
        return Ok(());
    }

    let has_encrypted_skill = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM workspace_skills WHERE auth_secret_enc IS NOT NULL)",
    )
    .fetch_one(db)
    .await?;
    if has_encrypted_skill
        && std::env::var("APP_SECRET_KEY")
            .map(|value| value.trim().is_empty())
            .unwrap_or(true)
    {
        return Err(anyhow::anyhow!(
            "APP_SECRET_KEY must be set because workspace_skills contains encrypted auth secrets"
        ));
    }

    Ok(())
}

fn webhook_secret_from_env(name: &'static str) -> anyhow::Result<String> {
    match std::env::var(name) {
        Ok(value) if !value.trim().is_empty() => Ok(value),
        _ => Err(anyhow::anyhow!(format!("{name} must be set"))),
    }
}

fn build_embedding_provider(config: &AppConfig) -> Arc<dyn EmbeddingProvider> {
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
            ))
        }
    }
}

fn build_llm_provider(config: &AppConfig) -> Arc<dyn LlmProvider> {
    match config.llm.provider {
        LlmProviderKind::Ollama => Arc::new(OllamaProvider::new(
            &config.llm.base_url,
            &config.llm.model,
            config.llm.timeout_secs,
        )),
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
            ))
        }
    }
}

async fn health() -> Json<Value> {
    Json(json!({ "status": "ok" }))
}

async fn readiness(State(state): State<AppState>) -> (StatusCode, Json<Value>) {
    let (database, redis, qdrant) = tokio::join!(
        check_database(&state.db),
        check_redis(&state.redis),
        check_qdrant()
    );
    let ready = database.is_ready() && redis.is_ready() && qdrant.is_ready();
    let status = if ready {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };

    (
        status,
        Json(json!({
            "status": if ready { "ok" } else { "unavailable" },
            "checks": {
                "database": database.as_str(),
                "redis": redis.as_str(),
                "qdrant": qdrant.as_str()
            }
        })),
    )
}

#[derive(Debug, Clone, Copy)]
enum DependencyStatus {
    Ok,
    MissingConfig,
    Unavailable,
}

impl DependencyStatus {
    fn is_ready(self) -> bool {
        matches!(self, DependencyStatus::Ok)
    }

    fn as_str(self) -> &'static str {
        match self {
            DependencyStatus::Ok => "ok",
            DependencyStatus::MissingConfig => "missing_config",
            DependencyStatus::Unavailable => "unavailable",
        }
    }
}

async fn check_database(pool: &sqlx::PgPool) -> DependencyStatus {
    match tokio::time::timeout(
        Duration::from_secs(2),
        sqlx::query("SELECT 1").execute(pool),
    )
    .await
    {
        Ok(Ok(_)) => DependencyStatus::Ok,
        Ok(Err(_)) | Err(_) => DependencyStatus::Unavailable,
    }
}

async fn check_redis(pool: &deadpool_redis::Pool) -> DependencyStatus {
    let Ok(mut connection) = pool.get().await else {
        return DependencyStatus::Unavailable;
    };

    match redis::cmd("PING")
        .query_async::<String>(&mut *connection)
        .await
    {
        Ok(_) => DependencyStatus::Ok,
        Err(_) => DependencyStatus::Unavailable,
    }
}

// ── Structured system health (/health/system) ─────────────────────────────

#[derive(Debug, Serialize, Deserialize)]
pub struct SystemHealthResponse {
    pub status: String,
    pub checks: Vec<HealthCheck>,
    pub checked_at: chrono::DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct HealthCheck {
    pub name: String,
    pub status: String,
    pub latency_ms: Option<u64>,
    pub message: Option<String>,
}

async fn system_health(State(state): axum::extract::State<AppState>) -> Json<SystemHealthResponse> {
    let config = state.config.clone();
    let db = state.db.clone();
    let redis = state.redis.clone();
    let qdrant = state.qdrant.clone();

    let (pg, rd, qd, ollama) = tokio::join!(
        probe_postgres(db),
        probe_redis(redis),
        probe_qdrant(qdrant),
        probe_ollama(config.clone()),
    );
    let fastembed = probe_fastembed(&config);

    let checks = vec![pg, rd, qd, ollama, fastembed];
    let overall = overall_health_status(&checks);

    Json(SystemHealthResponse {
        status: overall.to_owned(),
        checks,
        checked_at: Utc::now(),
    })
}

async fn probe_postgres(db: sqlx::PgPool) -> HealthCheck {
    let started = std::time::Instant::now();
    let result =
        tokio::time::timeout(Duration::from_secs(2), sqlx::query("SELECT 1").execute(&db)).await;
    let latency_ms = started.elapsed().as_millis() as u64;
    match result {
        Ok(Ok(_)) => HealthCheck {
            name: "postgres".to_owned(),
            status: "ok".to_owned(),
            latency_ms: Some(latency_ms),
            message: None,
        },
        Ok(Err(error)) => HealthCheck {
            name: "postgres".to_owned(),
            status: "error".to_owned(),
            latency_ms: Some(latency_ms),
            message: Some(error.to_string()),
        },
        Err(_) => HealthCheck {
            name: "postgres".to_owned(),
            status: "error".to_owned(),
            latency_ms: Some(2000),
            message: Some("timeout".to_owned()),
        },
    }
}

async fn probe_redis(redis: deadpool_redis::Pool) -> HealthCheck {
    let started = std::time::Instant::now();
    let mut connection = match redis.get().await {
        Ok(connection) => connection,
        Err(error) => {
            return HealthCheck {
                name: "redis".to_owned(),
                status: "error".to_owned(),
                latency_ms: Some(started.elapsed().as_millis() as u64),
                message: Some(error.to_string()),
            }
        }
    };
    let result = tokio::time::timeout(
        Duration::from_secs(2),
        redis::cmd("PING").query_async::<String>(&mut *connection),
    )
    .await;
    let latency_ms = started.elapsed().as_millis() as u64;
    match result {
        Ok(Ok(_)) => HealthCheck {
            name: "redis".to_owned(),
            status: "ok".to_owned(),
            latency_ms: Some(latency_ms),
            message: None,
        },
        Ok(Err(error)) => HealthCheck {
            name: "redis".to_owned(),
            status: "error".to_owned(),
            latency_ms: Some(latency_ms),
            message: Some(error.to_string()),
        },
        Err(_) => HealthCheck {
            name: "redis".to_owned(),
            status: "error".to_owned(),
            latency_ms: Some(2000),
            message: Some("timeout".to_owned()),
        },
    }
}

async fn probe_qdrant(qdrant: Qdrant) -> HealthCheck {
    let started = std::time::Instant::now();
    let result = tokio::time::timeout(Duration::from_secs(2), qdrant.health_check()).await;
    let latency_ms = started.elapsed().as_millis() as u64;
    match result {
        Ok(Ok(_)) => HealthCheck {
            name: "qdrant".to_owned(),
            status: "ok".to_owned(),
            latency_ms: Some(latency_ms),
            message: None,
        },
        Ok(Err(error)) => HealthCheck {
            name: "qdrant".to_owned(),
            status: "error".to_owned(),
            latency_ms: Some(latency_ms),
            message: Some(error.to_string()),
        },
        Err(_) => HealthCheck {
            name: "qdrant".to_owned(),
            status: "error".to_owned(),
            latency_ms: Some(2000),
            message: Some("timeout".to_owned()),
        },
    }
}

async fn probe_ollama(config: Arc<common::config::AppConfig>) -> HealthCheck {
    use common::config::LlmProviderKind;
    if config.llm.provider != LlmProviderKind::Ollama {
        return HealthCheck {
            name: "ollama".to_owned(),
            status: "ok".to_owned(),
            latency_ms: None,
            message: Some("not configured".to_owned()),
        };
    }
    let url = format!("{}/api/tags", config.llm.base_url.trim_end_matches('/'));
    let started = std::time::Instant::now();
    let result = tokio::time::timeout(Duration::from_secs(2), reqwest::get(&url)).await;
    let latency_ms = started.elapsed().as_millis() as u64;
    match result {
        Ok(Ok(resp)) if resp.status().is_success() => HealthCheck {
            name: "ollama".to_owned(),
            status: "ok".to_owned(),
            latency_ms: Some(latency_ms),
            message: None,
        },
        Ok(Ok(resp)) => HealthCheck {
            name: "ollama".to_owned(),
            status: "warn".to_owned(),
            latency_ms: Some(latency_ms),
            message: Some(format!("HTTP {}", resp.status())),
        },
        Ok(Err(error)) => HealthCheck {
            name: "ollama".to_owned(),
            status: "error".to_owned(),
            latency_ms: Some(latency_ms),
            message: Some(error.to_string()),
        },
        Err(_) => HealthCheck {
            name: "ollama".to_owned(),
            status: "error".to_owned(),
            latency_ms: Some(2000),
            message: Some("timeout".to_owned()),
        },
    }
}

fn overall_health_status(checks: &[HealthCheck]) -> &'static str {
    // An empty check list is considered healthy (no dependencies configured).
    if checks.iter().any(|c| c.status == "error") {
        "unhealthy"
    } else if checks.iter().any(|c| c.status == "warn") {
        "degraded"
    } else {
        "healthy"
    }
}

fn probe_fastembed(config: &AppConfig) -> HealthCheck {
    use common::config::EmbeddingProviderKind;
    if config.embedding.provider != EmbeddingProviderKind::FastEmbed {
        return HealthCheck {
            name: "fastembed".to_owned(),
            status: "ok".to_owned(),
            latency_ms: None,
            message: Some("not configured".to_owned()),
        };
    }
    // FastEmbed is always in-process; if the binary is running, the model is loaded.
    HealthCheck {
        name: "fastembed".to_owned(),
        status: "ok".to_owned(),
        latency_ms: None,
        message: None,
    }
}

// ──────────────────────────────────────────────────────────────────────────────

async fn check_qdrant() -> DependencyStatus {
    let Ok(qdrant_url) = std::env::var("QDRANT_URL") else {
        return DependencyStatus::MissingConfig;
    };

    let Ok(client) = Qdrant::from_url(&qdrant_url).build() else {
        return DependencyStatus::Unavailable;
    };

    match client.health_check().await {
        Ok(_) => DependencyStatus::Ok,
        Err(_) => DependencyStatus::Unavailable,
    }
}

#[cfg(test)]
mod tests {
    use std::{
        sync::Arc,
        time::{SystemTime, UNIX_EPOCH},
    };

    use axum::{
        body::{to_bytes, Body},
        http::{Method, Request, StatusCode},
    };
    use chrono::{DateTime, Duration, SecondsFormat, Utc};
    use common::models::{MemoryType, ScopeVisibility};
    use serde_json::{json, Value};
    use sqlx::PgPool;
    use tower::ServiceExt;
    use uuid::Uuid;

    use super::*;

    async fn test_state(pool: PgPool) -> AppState {
        let redis_url =
            std::env::var("REDIS_URL").unwrap_or_else(|_| "redis://localhost:16379".to_owned());
        let redis = {
            let cfg = deadpool_redis::Config::from_url(&redis_url);
            match cfg.create_pool(Some(deadpool_redis::Runtime::Tokio1)) {
                Ok(pool) => pool,
                Err(error) => panic!("test Redis pool should be created: {error}"),
            }
        };
        let qdrant_url =
            std::env::var("QDRANT_URL").unwrap_or_else(|_| "http://localhost:16333".to_owned());
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
            embedding_provider: Arc::new(FastEmbedProvider::new("test-embedding")),
            llm_provider: Arc::new(OllamaProvider::new("http://127.0.0.1:9", "test-llm", 1)),
            config: Arc::new(config),
            github_webhook_secret: "test-secret".to_owned(),
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
            panic!("test workspace insert should succeed: {error}");
        }

        workspace_id
    }

    async fn insert_api_key(pool: &PgPool, workspace_id: Uuid, revoked: bool) -> String {
        let key_id = Uuid::now_v7();
        let (plaintext, prefix) = security::generate_api_key(workspace_id);
        let key_hash = match security::hash_secret(&plaintext) {
            Ok(hash) => hash,
            Err(error) => panic!("test key hash should be generated: {error}"),
        };
        let revoked_at = if revoked { Some(Utc::now()) } else { None };
        let result = sqlx::query(
            r#"
            INSERT INTO api_keys (
                id, workspace_id, name, key_hash, prefix, prefix_version, revoked, revoked_at
            )
            VALUES ($1, $2, $3, $4, $5, 2, $6, $7)
            "#,
        )
        .bind(key_id)
        .bind(workspace_id)
        .bind("test key")
        .bind(key_hash)
        .bind(prefix)
        .bind(revoked)
        .bind(revoked_at)
        .execute(pool)
        .await;

        if let Err(error) = result {
            panic!("test API key insert should succeed: {error}");
        }

        plaintext
    }

    async fn insert_memory(pool: &PgPool, workspace_id: Uuid, content: &str) -> Uuid {
        insert_memory_with_repo(pool, workspace_id, content, "Quazmoz/memoryops").await
    }

    async fn insert_memory_with_repo(
        pool: &PgPool,
        workspace_id: Uuid,
        content: &str,
        repo: &str,
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
                tags
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            "#,
        )
        .bind(memory_id)
        .bind(workspace_id)
        .bind(json!({
            "workspace_id": workspace_id,
            "agent_id": null,
            "user_id": null,
            "repo": repo
        }))
        .bind(MemoryType::Episodic)
        .bind(content)
        .bind(json!([]))
        .bind(0.9_f32)
        .bind(Vec::<String>::new())
        .execute(pool)
        .await;

        if let Err(error) = result {
            panic!("test memory insert should succeed: {error}");
        }

        memory_id
    }

    struct MemoryFixture<'a> {
        content: &'a str,
        memory_type: MemoryType,
        scope_visibility: ScopeVisibility,
        agent_id: Option<&'a str>,
        repo: Option<&'a str>,
        importance_score: f32,
        created_at: DateTime<Utc>,
        deleted_at: Option<DateTime<Utc>>,
    }

    async fn insert_memory_fixture(
        pool: &PgPool,
        workspace_id: Uuid,
        fixture: MemoryFixture<'_>,
    ) -> Uuid {
        let memory_id = Uuid::now_v7();
        let result = sqlx::query(
            r#"
            INSERT INTO memory_units (
                id,
                workspace_id,
                scope,
                memory_type,
                scope_visibility,
                content,
                entities,
                importance_score,
                decay_score,
                tags,
                created_at,
                updated_at,
                deleted_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $11, $12)
            "#,
        )
        .bind(memory_id)
        .bind(workspace_id)
        .bind(json!({
            "workspace_id": workspace_id,
            "agent_id": fixture.agent_id,
            "user_id": null,
            "repo": fixture.repo
        }))
        .bind(fixture.memory_type)
        .bind(fixture.scope_visibility)
        .bind(fixture.content)
        .bind(json!([]))
        .bind(fixture.importance_score)
        .bind(fixture.importance_score)
        .bind(Vec::<String>::new())
        .bind(fixture.created_at)
        .bind(fixture.deleted_at)
        .execute(pool)
        .await;

        if let Err(error) = result {
            panic!("test memory fixture insert should succeed: {error}");
        }

        memory_id
    }

    async fn insert_memory_version(
        pool: &PgPool,
        workspace_id: Uuid,
        memory_id: Uuid,
        version: i32,
        content: &str,
        importance_score: f32,
        created_at: DateTime<Utc>,
    ) {
        let result = sqlx::query(
            r#"
            INSERT INTO memory_versions (
                id, memory_id, workspace_id, version, content, importance_score, tags, edited_by, created_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
            "#,
        )
        .bind(Uuid::now_v7())
        .bind(memory_id)
        .bind(workspace_id)
        .bind(version)
        .bind(content)
        .bind(importance_score)
        .bind(Vec::<String>::new())
        .bind("test")
        .bind(created_at)
        .execute(pool)
        .await;

        if let Err(error) = result {
            panic!("test memory version insert should succeed: {error}");
        }
    }

    fn utc_query_timestamp(value: DateTime<Utc>) -> String {
        value.to_rfc3339_opts(SecondsFormat::Secs, true)
    }

    async fn wait_for_publish_audit(pool: &PgPool, workspace_id: Uuid, memory_id: Uuid) -> i64 {
        let started = std::time::Instant::now();
        let budget = std::time::Duration::from_secs(3);
        let mut delay = std::time::Duration::from_millis(10);
        loop {
            let count = match sqlx::query_scalar::<_, i64>(
                r#"
                SELECT COUNT(*)
                FROM audit_log
                WHERE workspace_id = $1
                  AND target_id = $2
                  AND action::text = 'publish'
                "#,
            )
            .bind(workspace_id)
            .bind(memory_id)
            .fetch_one(pool)
            .await
            {
                Ok(count) => count,
                Err(error) => panic!("audit count query should succeed: {error}"),
            };

            if count > 0 {
                return count;
            }

            let elapsed = started.elapsed();
            if elapsed >= budget {
                return 0;
            }

            let remaining = budget.saturating_sub(elapsed);
            tokio::time::sleep(delay.min(remaining)).await;
            delay = (delay * 2).min(std::time::Duration::from_millis(500));
        }
    }

    fn request(method: Method, uri: String, api_key: Option<&str>, body: Value) -> Request<Body> {
        let mut builder = Request::builder()
            .method(method)
            .uri(uri)
            .header("content-type", "application/json");

        if let Some(api_key) = api_key {
            builder = builder.header("x-api-key", api_key);
        }

        match builder.body(Body::from(body.to_string())) {
            Ok(request) => request,
            Err(error) => panic!("test request should build: {error}"),
        }
    }

    async fn response_json(response: axum::response::Response) -> Value {
        let bytes = match to_bytes(response.into_body(), usize::MAX).await {
            Ok(bytes) => bytes,
            Err(error) => panic!("response body should be readable: {error}"),
        };
        match serde_json::from_slice::<Value>(&bytes) {
            Ok(value) => value,
            Err(error) => panic!("response body should be JSON: {error}"),
        }
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn bootstrap_flow_creates_key_and_lists_memory(pool: PgPool) {
        let app = router(test_state(pool).await);
        let workspace_name = format!("workspace-{}", Uuid::now_v7());
        let response = match app
            .clone()
            .oneshot(request(
                Method::POST,
                "/v1/workspaces".to_owned(),
                None,
                json!({ "name": workspace_name }),
            ))
            .await
        {
            Ok(response) => response,
            Err(error) => panic!("workspace request should respond: {error}"),
        };
        assert_eq!(response.status(), StatusCode::OK);
        let body = response_json(response).await;
        let _workspace_id = match body.get("workspace_id").and_then(Value::as_str) {
            Some(raw) => match Uuid::parse_str(raw) {
                Ok(workspace_id) => workspace_id,
                Err(error) => panic!("workspace_id should be a UUID: {error}"),
            },
            None => panic!("workspace response should include workspace_id"),
        };
        let api_key = match body.get("api_key").and_then(Value::as_str) {
            Some(key) if !key.trim().is_empty() => key.to_owned(),
            Some(_) => panic!("workspace response api_key should be non-empty"),
            None => panic!("workspace response should include api_key"),
        };

        let response = match app
            .oneshot(request(
                Method::GET,
                "/v1/memory".to_owned(),
                Some(&api_key),
                json!(null),
            ))
            .await
        {
            Ok(response) => response,
            Err(error) => panic!("memory list should respond: {error}"),
        };

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn missing_key_returns_401(pool: PgPool) {
        let app = router(test_state(pool).await);
        let response = match app
            .oneshot(request(
                Method::GET,
                "/v1/memory".to_owned(),
                None,
                json!(null),
            ))
            .await
        {
            Ok(response) => response,
            Err(error) => panic!("request should respond: {error}"),
        };

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn retrieve_packs_memory_within_budget_and_persists_trace(pool: PgPool) {
        let workspace_id = insert_workspace(&pool).await;
        let api_key = insert_api_key(&pool, workspace_id, false).await;
        let memory_id = insert_memory(&pool, workspace_id, "alpha").await;
        let app = router(test_state(pool).await);

        let response = match app
            .clone()
            .oneshot(request(
                Method::POST,
                "/v1/retrieve".to_owned(),
                Some(&api_key),
                json!({
                    "query": "alpha",
                    "workspace_id": workspace_id,
                    "token_budget": 1,
                    "mode": "keyword",
                    "include_trace": false
                }),
            ))
            .await
        {
            Ok(response) => response,
            Err(error) => panic!("retrieve request should respond: {error}"),
        };
        assert_eq!(response.status(), StatusCode::OK);
        let body = response_json(response).await;
        let total_tokens = body
            .get("total_tokens")
            .and_then(Value::as_u64)
            .unwrap_or(u64::MAX);
        assert!(total_tokens <= 1);
        assert!(body.get("trace").is_some_and(Value::is_null));
        let memory_id_text = memory_id.to_string();
        assert_eq!(
            body.get("memories")
                .and_then(Value::as_array)
                .and_then(|memories| memories.first())
                .and_then(|memory| memory.get("id"))
                .and_then(Value::as_str),
            Some(memory_id_text.as_str())
        );
        let query_id = match body.get("query_id").and_then(Value::as_str) {
            Some(raw) => match Uuid::parse_str(raw) {
                Ok(query_id) => query_id,
                Err(error) => panic!("query_id should be a UUID: {error}"),
            },
            None => panic!("retrieve response should include query_id"),
        };

        let response = match app
            .oneshot(request(
                Method::GET,
                format!("/v1/retrieve/trace/{query_id}"),
                Some(&api_key),
                json!(null),
            ))
            .await
        {
            Ok(response) => response,
            Err(error) => panic!("trace request should respond: {error}"),
        };
        assert_eq!(response.status(), StatusCode::OK);
        let trace = response_json(response).await;
        let query_id_text = query_id.to_string();
        assert_eq!(
            trace.get("query_id").and_then(Value::as_str),
            Some(query_id_text.as_str())
        );
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn retrieve_scope_filter_isolates_by_repo(pool: PgPool) {
        let workspace_id = insert_workspace(&pool).await;
        let api_key = insert_api_key(&pool, workspace_id, false).await;
        let memory_a = insert_memory_with_repo(
            &pool,
            workspace_id,
            "alpha deployment memory for repo scope",
            "Quazmoz/memoryops",
        )
        .await;
        let memory_b = insert_memory_with_repo(
            &pool,
            workspace_id,
            "alpha deployment memory for other repo",
            "Quazmoz/other",
        )
        .await;
        let app = router(test_state(pool).await);

        let response = match app
            .oneshot(request(
                Method::POST,
                "/v1/retrieve".to_owned(),
                Some(&api_key),
                json!({
                    "query": "alpha deployment memory",
                    "workspace_id": workspace_id,
                    "mode": "keyword",
                    "scope": { "repo": "Quazmoz/memoryops" },
                    "token_budget": 8096
                }),
            ))
            .await
        {
            Ok(response) => response,
            Err(error) => panic!("scoped retrieve request should respond: {error}"),
        };

        assert_eq!(response.status(), StatusCode::OK);
        let body = response_json(response).await;
        let memories = match body.get("memories").and_then(Value::as_array) {
            Some(memories) => memories,
            None => panic!("retrieve response should include memories array"),
        };
        let memory_a_text = memory_a.to_string();
        let memory_b_text = memory_b.to_string();

        assert_eq!(memories.len(), 1);
        assert!(memories.iter().any(|memory| {
            memory.get("id").and_then(Value::as_str) == Some(memory_a_text.as_str())
        }));
        assert!(!memories.iter().any(|memory| {
            memory.get("id").and_then(Value::as_str) == Some(memory_b_text.as_str())
        }));
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn memory_list_as_of_reconstructs_historical_state(pool: PgPool) {
        let workspace_id = insert_workspace(&pool).await;
        let api_key = insert_api_key(&pool, workspace_id, false).await;
        let as_of = Utc::now() - Duration::days(10);
        let before_as_of = as_of - Duration::days(10);
        let after_as_of = as_of + Duration::days(1);
        let historical_id = insert_memory_fixture(
            &pool,
            workspace_id,
            MemoryFixture {
                content: "current incident summary",
                memory_type: MemoryType::Semantic,
                scope_visibility: ScopeVisibility::Private,
                agent_id: None,
                repo: Some("Quazmoz/memoryops"),
                importance_score: 0.9,
                created_at: before_as_of,
                deleted_at: None,
            },
        )
        .await;
        insert_memory_version(
            &pool,
            workspace_id,
            historical_id,
            1,
            "historical incident summary",
            0.8,
            before_as_of,
        )
        .await;
        insert_memory_version(
            &pool,
            workspace_id,
            historical_id,
            2,
            "future incident summary",
            0.95,
            after_as_of,
        )
        .await;
        insert_memory_fixture(
            &pool,
            workspace_id,
            MemoryFixture {
                content: "future-only incident summary",
                memory_type: MemoryType::Semantic,
                scope_visibility: ScopeVisibility::Private,
                agent_id: None,
                repo: Some("Quazmoz/memoryops"),
                importance_score: 0.9,
                created_at: after_as_of,
                deleted_at: None,
            },
        )
        .await;
        insert_memory_fixture(
            &pool,
            workspace_id,
            MemoryFixture {
                content: "already deleted incident summary",
                memory_type: MemoryType::Semantic,
                scope_visibility: ScopeVisibility::Private,
                agent_id: None,
                repo: Some("Quazmoz/memoryops"),
                importance_score: 0.9,
                created_at: before_as_of,
                deleted_at: Some(as_of - Duration::days(1)),
            },
        )
        .await;
        let app = router(test_state(pool).await);

        let response = match app
            .oneshot(request(
                Method::GET,
                format!("/v1/memory?as_of={}", utc_query_timestamp(as_of)),
                Some(&api_key),
                json!(null),
            ))
            .await
        {
            Ok(response) => response,
            Err(error) => panic!("as_of memory list should respond: {error}"),
        };

        assert_eq!(response.status(), StatusCode::OK);
        let body = response_json(response).await;
        let items = match body.get("items").and_then(Value::as_array) {
            Some(items) => items,
            None => panic!("memory list response should include items"),
        };
        let historical_id_text = historical_id.to_string();
        assert_eq!(items.len(), 1);
        assert_eq!(
            items[0].get("id").and_then(Value::as_str),
            Some(historical_id_text.as_str())
        );
        assert_eq!(
            items[0].get("content").and_then(Value::as_str),
            Some("historical incident summary")
        );
        let decay_score = items[0]
            .get("decay_score")
            .and_then(Value::as_f64)
            .unwrap_or(0.0);
        assert!(decay_score < 0.8);
        assert!(decay_score > 0.6);
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn retrieve_as_of_filters_future_memories(pool: PgPool) {
        let workspace_id = insert_workspace(&pool).await;
        let api_key = insert_api_key(&pool, workspace_id, false).await;
        let as_of = Utc::now() - Duration::days(5);
        let included_id = insert_memory_fixture(
            &pool,
            workspace_id,
            MemoryFixture {
                content: "alpha incident mitigation existed",
                memory_type: MemoryType::Episodic,
                scope_visibility: ScopeVisibility::Private,
                agent_id: None,
                repo: Some("Quazmoz/memoryops"),
                importance_score: 0.9,
                created_at: as_of - Duration::days(1),
                deleted_at: None,
            },
        )
        .await;
        let excluded_id = insert_memory_fixture(
            &pool,
            workspace_id,
            MemoryFixture {
                content: "alpha incident mitigation future",
                memory_type: MemoryType::Episodic,
                scope_visibility: ScopeVisibility::Private,
                agent_id: None,
                repo: Some("Quazmoz/memoryops"),
                importance_score: 0.9,
                created_at: as_of + Duration::days(1),
                deleted_at: None,
            },
        )
        .await;
        let app = router(test_state(pool).await);

        let response = match app
            .oneshot(request(
                Method::POST,
                "/v1/retrieve".to_owned(),
                Some(&api_key),
                json!({
                    "query": "alpha incident mitigation",
                    "workspace_id": workspace_id,
                    "mode": "keyword",
                    "as_of": utc_query_timestamp(as_of),
                    "token_budget": 8096,
                    "include_trace": true
                }),
            ))
            .await
        {
            Ok(response) => response,
            Err(error) => panic!("as_of retrieve should respond: {error}"),
        };

        assert_eq!(response.status(), StatusCode::OK);
        let body = response_json(response).await;
        let memories = match body.get("memories").and_then(Value::as_array) {
            Some(memories) => memories,
            None => panic!("retrieve response should include memories"),
        };
        let included_id_text = included_id.to_string();
        let excluded_id_text = excluded_id.to_string();
        assert!(memories.iter().any(|memory| {
            memory.get("id").and_then(Value::as_str) == Some(included_id_text.as_str())
        }));
        assert!(!memories.iter().any(|memory| {
            memory.get("id").and_then(Value::as_str) == Some(excluded_id_text.as_str())
        }));
        let expected_as_of = utc_query_timestamp(as_of);
        assert_eq!(
            body.get("trace")
                .and_then(|trace| trace.get("as_of"))
                .and_then(Value::as_str),
            Some(expected_as_of.as_str())
        );
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn publish_endpoint_rejects_episodic_memory(pool: PgPool) {
        let workspace_id = insert_workspace(&pool).await;
        let api_key = insert_api_key(&pool, workspace_id, false).await;
        let memory_id = insert_memory_fixture(
            &pool,
            workspace_id,
            MemoryFixture {
                content: "episodic memory cannot publish",
                memory_type: MemoryType::Episodic,
                scope_visibility: ScopeVisibility::Private,
                agent_id: Some("agent-a"),
                repo: Some("Quazmoz/memoryops"),
                importance_score: 0.9,
                created_at: Utc::now(),
                deleted_at: None,
            },
        )
        .await;
        let app = router(test_state(pool).await);

        let response = match app
            .oneshot(request(
                Method::POST,
                format!("/v1/memory/{memory_id}/publish?workspace_id={workspace_id}"),
                Some(&api_key),
                json!(null),
            ))
            .await
        {
            Ok(response) => response,
            Err(error) => panic!("publish request should respond: {error}"),
        };

        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn publish_sets_workspace_visibility_and_writes_audit(pool: PgPool) {
        let workspace_id = insert_workspace(&pool).await;
        let api_key = insert_api_key(&pool, workspace_id, false).await;
        let memory_id = insert_memory_fixture(
            &pool,
            workspace_id,
            MemoryFixture {
                content: "semantic memory can publish",
                memory_type: MemoryType::Semantic,
                scope_visibility: ScopeVisibility::Private,
                agent_id: Some("agent-a"),
                repo: Some("Quazmoz/memoryops"),
                importance_score: 0.9,
                created_at: Utc::now(),
                deleted_at: None,
            },
        )
        .await;
        let check_pool = pool.clone();
        let app = router(test_state(pool).await);

        let response = match app
            .oneshot(request(
                Method::POST,
                format!("/v1/memory/{memory_id}/publish?workspace_id={workspace_id}"),
                Some(&api_key),
                json!(null),
            ))
            .await
        {
            Ok(response) => response,
            Err(error) => panic!("publish request should respond: {error}"),
        };

        assert_eq!(response.status(), StatusCode::OK);
        let body = response_json(response).await;
        assert_eq!(
            body.get("scope_visibility").and_then(Value::as_str),
            Some("workspace")
        );
        let audit_count = wait_for_publish_audit(&check_pool, workspace_id, memory_id).await;
        assert_eq!(audit_count, 1);
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn published_memory_searches_across_agents_when_pool_included(pool: PgPool) {
        let workspace_id = insert_workspace(&pool).await;
        let api_key = insert_api_key(&pool, workspace_id, false).await;
        let memory_id = insert_memory_fixture(
            &pool,
            workspace_id,
            MemoryFixture {
                content: "shared alpha deployment runbook",
                memory_type: MemoryType::Semantic,
                scope_visibility: ScopeVisibility::Workspace,
                agent_id: Some("agent-a"),
                repo: Some("Quazmoz/memoryops"),
                importance_score: 0.9,
                created_at: Utc::now(),
                deleted_at: None,
            },
        )
        .await;
        let app = router(test_state(pool).await);

        let response = match app
            .oneshot(request(
                Method::POST,
                "/v1/memory/search".to_owned(),
                Some(&api_key),
                json!({
                    "query": "shared alpha deployment",
                    "workspace_id": workspace_id,
                    "mode": "keyword",
                    "agent_id": "agent-b",
                    "include_workspace_pool": true
                }),
            ))
            .await
        {
            Ok(response) => response,
            Err(error) => panic!("workspace-pool search should respond: {error}"),
        };

        assert_eq!(response.status(), StatusCode::OK);
        let body = response_json(response).await;
        let results = match body.get("results").and_then(Value::as_array) {
            Some(results) => results,
            None => panic!("search response should include results"),
        };
        let memory_id_text = memory_id.to_string();
        assert!(results.iter().any(|result| {
            result
                .get("memory")
                .and_then(|memory| memory.get("id"))
                .and_then(Value::as_str)
                == Some(memory_id_text.as_str())
        }));
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn sub_agent_pool_config_inherits_published_retrieval_memories(pool: PgPool) {
        let workspace_id = insert_workspace(&pool).await;
        let result = sqlx::query("UPDATE workspaces SET config = $2 WHERE id = $1")
            .bind(workspace_id)
            .bind(json!({ "sub_agent_pools": ["agent-a"] }))
            .execute(&pool)
            .await;
        if let Err(error) = result {
            panic!("workspace config update should succeed: {error}");
        }
        let api_key = insert_api_key(&pool, workspace_id, false).await;
        let inherited_id = insert_memory_fixture(
            &pool,
            workspace_id,
            MemoryFixture {
                content: "inherited alpha workspace pool memory",
                memory_type: MemoryType::Semantic,
                scope_visibility: ScopeVisibility::Workspace,
                agent_id: Some("agent-a"),
                repo: Some("Quazmoz/memoryops"),
                importance_score: 0.9,
                created_at: Utc::now(),
                deleted_at: None,
            },
        )
        .await;
        let private_id = insert_memory_fixture(
            &pool,
            workspace_id,
            MemoryFixture {
                content: "inherited alpha private memory",
                memory_type: MemoryType::Semantic,
                scope_visibility: ScopeVisibility::Private,
                agent_id: Some("agent-a"),
                repo: Some("Quazmoz/memoryops"),
                importance_score: 0.9,
                created_at: Utc::now(),
                deleted_at: None,
            },
        )
        .await;
        let app = router(test_state(pool).await);

        let response = match app
            .oneshot(request(
                Method::POST,
                "/v1/retrieve".to_owned(),
                Some(&api_key),
                json!({
                    "query": "inherited alpha",
                    "workspace_id": workspace_id,
                    "mode": "keyword",
                    "agent_id": "agent-b",
                    "token_budget": 8096
                }),
            ))
            .await
        {
            Ok(response) => response,
            Err(error) => panic!("sub-agent pool retrieve should respond: {error}"),
        };

        assert_eq!(response.status(), StatusCode::OK);
        let body = response_json(response).await;
        let memories = match body.get("memories").and_then(Value::as_array) {
            Some(memories) => memories,
            None => panic!("retrieve response should include memories"),
        };
        let inherited_id_text = inherited_id.to_string();
        let private_id_text = private_id.to_string();
        assert!(memories.iter().any(|memory| {
            memory.get("id").and_then(Value::as_str) == Some(inherited_id_text.as_str())
        }));
        assert!(!memories.iter().any(|memory| {
            memory.get("id").and_then(Value::as_str) == Some(private_id_text.as_str())
        }));
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn revoked_key_returns_401(pool: PgPool) {
        let workspace_id = insert_workspace(&pool).await;
        let api_key = insert_api_key(&pool, workspace_id, true).await;
        let app = router(test_state(pool).await);
        let response = match app
            .oneshot(request(
                Method::GET,
                "/v1/memory".to_owned(),
                Some(&api_key),
                json!(null),
            ))
            .await
        {
            Ok(response) => response,
            Err(error) => panic!("request should respond: {error}"),
        };

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn wrong_workspace_key_returns_403(pool: PgPool) {
        let key_workspace_id = insert_workspace(&pool).await;
        let target_workspace_id = insert_workspace(&pool).await;
        let api_key = insert_api_key(&pool, key_workspace_id, false).await;
        let app = router(test_state(pool).await);
        let response = match app
            .oneshot(request(
                Method::GET,
                format!("/v1/workspaces/{target_workspace_id}"),
                Some(&api_key),
                json!(null),
            ))
            .await
        {
            Ok(response) => response,
            Err(error) => panic!("request should respond: {error}"),
        };

        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn workspace_config_rejects_zero_decay_half_life(pool: PgPool) {
        let workspace_id = insert_workspace(&pool).await;
        let api_key = insert_api_key(&pool, workspace_id, false).await;
        let app = router(test_state(pool).await);
        let response = match app
            .oneshot(request(
                Method::PATCH,
                format!("/v1/workspaces/{workspace_id}/config"),
                Some(&api_key),
                json!({ "decay_half_life_days": 0 }),
            ))
            .await
        {
            Ok(response) => response,
            Err(error) => panic!("request should respond: {error}"),
        };

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn workspace_config_rejects_high_pruning_threshold(pool: PgPool) {
        let workspace_id = insert_workspace(&pool).await;
        let api_key = insert_api_key(&pool, workspace_id, false).await;
        let app = router(test_state(pool).await);
        let response = match app
            .oneshot(request(
                Method::PATCH,
                format!("/v1/workspaces/{workspace_id}/config"),
                Some(&api_key),
                json!({ "pruning_threshold": 0.99 }),
            ))
            .await
        {
            Ok(response) => response,
            Err(error) => panic!("request should respond: {error}"),
        };

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn workspace_config_accepts_lifecycle_values(pool: PgPool) {
        let workspace_id = insert_workspace(&pool).await;
        let api_key = insert_api_key(&pool, workspace_id, false).await;
        let app = router(test_state(pool).await);
        let response = match app
            .oneshot(request(
                Method::PATCH,
                format!("/v1/workspaces/{workspace_id}/config"),
                Some(&api_key),
                json!({
                    "decay_half_life_days": 90,
                    "pruning_threshold": 0.15
                }),
            ))
            .await
        {
            Ok(response) => response,
            Err(error) => panic!("request should respond: {error}"),
        };

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn rate_limit_exceeded_returns_429(pool: PgPool) {
        let workspace_id = insert_workspace(&pool).await;
        let api_key = insert_api_key(&pool, workspace_id, false).await;
        let state = test_state(pool).await;
        seed_rate_limit(&state.redis, workspace_id).await;
        let app = router(state);
        let response = match app
            .oneshot(request(
                Method::GET,
                "/v1/memory".to_owned(),
                Some(&api_key),
                json!(null),
            ))
            .await
        {
            Ok(response) => response,
            Err(error) => panic!("request should respond: {error}"),
        };

        assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
    }

    fn ok_check(name: &str) -> HealthCheck {
        HealthCheck {
            name: name.to_owned(),
            status: "ok".to_owned(),
            latency_ms: Some(1),
            message: None,
        }
    }

    fn warn_check(name: &str) -> HealthCheck {
        HealthCheck {
            name: name.to_owned(),
            status: "warn".to_owned(),
            latency_ms: Some(1),
            message: None,
        }
    }

    fn error_check(name: &str) -> HealthCheck {
        HealthCheck {
            name: name.to_owned(),
            status: "error".to_owned(),
            latency_ms: Some(1),
            message: Some("down".to_owned()),
        }
    }

    #[test]
    fn health_status_all_ok_is_healthy() {
        let checks = vec![ok_check("postgres"), ok_check("redis"), ok_check("qdrant")];
        assert_eq!(overall_health_status(&checks), "healthy");
    }

    #[test]
    fn health_status_one_warn_is_degraded() {
        let checks = vec![
            ok_check("postgres"),
            warn_check("ollama"),
            ok_check("qdrant"),
        ];
        assert_eq!(overall_health_status(&checks), "degraded");
    }

    #[test]
    fn health_status_one_error_is_unhealthy() {
        let checks = vec![
            ok_check("postgres"),
            error_check("redis"),
            ok_check("qdrant"),
        ];
        assert_eq!(overall_health_status(&checks), "unhealthy");
    }

    #[test]
    fn health_status_error_wins_over_warn() {
        let checks = vec![
            warn_check("ollama"),
            error_check("redis"),
            ok_check("qdrant"),
        ];
        assert_eq!(overall_health_status(&checks), "unhealthy");
    }

    #[test]
    fn health_status_empty_checks_is_healthy() {
        assert_eq!(overall_health_status(&[]), "healthy");
    }

    async fn seed_rate_limit(redis: &deadpool_redis::Pool, workspace_id: Uuid) {
        let now = match SystemTime::now().duration_since(UNIX_EPOCH) {
            Ok(duration) => i64::try_from(duration.as_secs()).unwrap_or(0),
            Err(error) => panic!("system time should be after epoch: {error}"),
        };
        let window_start = now - (now % 60);
        let mut connection = match redis.get().await {
            Ok(conn) => conn,
            Err(error) => panic!("test redis connection should succeed: {error}"),
        };
        for window in [window_start] {
            let key = format!("rate:workspace:{workspace_id}:memory:{window}");
            let result = redis::cmd("SET")
                .arg(key)
                .arg(crate::middleware::rate_limit::MEMORY_RPM + 1)
                .query_async::<()>(&mut *connection)
                .await;
            if let Err(error) = result {
                panic!("rate limit seed should succeed: {error}");
            }
        }
    }
}
