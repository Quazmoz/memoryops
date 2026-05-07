use std::{net::SocketAddr, sync::Arc, time::Duration};

use anyhow::anyhow;
use axum::{
    extract::State, http::StatusCode, middleware as axum_middleware, routing::get, Json, Router,
};
use chrono::Utc;
use common::{
    build_embedding_provider, build_llm_provider, config::AppConfig, telemetry::init_telemetry,
    AppState,
};
use qdrant_client::Qdrant;
use retrieval::retrieval_router;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::postgres::PgPoolOptions;
use tokio::sync::Semaphore;

#[cfg(test)]
use common::providers::{FastEmbedProvider, OllamaProvider};

mod handlers;
mod middleware;
mod security;

/// The dev-only placeholder value that must never reach production.
const DEV_PLACEHOLDER: &str = "dev-placeholder";

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config_path = std::env::var("CONFIG_PATH").unwrap_or_else(|_| "config.toml".to_owned());
    let config = AppConfig::from_path(config_path)?;
    let _telemetry_guard = init_telemetry(&config.telemetry)?;
    crate::security::validate_secret_key_at_startup()
        .map_err(|_| anyhow!("APP_SECRET_KEY is missing or invalid -- cannot start"))?;
    workspace_creation_secret_from_env()?;
    validate_production_secrets()?;
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
    let trusted_proxy_cidrs = Arc::new(parse_trusted_proxy_cidrs());

    let db = PgPoolOptions::new()
        .max_connections(config.database.max_connections)
        .min_connections(config.database.min_connections)
        .acquire_timeout(Duration::from_secs(config.database.connect_timeout_secs))
        .connect(&database_url)
        .await?;
    ensure_skill_secret_configuration(&db).await?;
    #[allow(clippy::expect_used)]
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
        trusted_proxy_cidrs,
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

/// When `APP_ENV=production`, reject secrets that are empty or still set to
/// the dev-only placeholder value.  This prevents accidental production
/// deployments with insecure defaults.
fn validate_production_secrets() -> anyhow::Result<()> {
    let is_production = std::env::var("APP_ENV")
        .map(|v| v.trim().eq_ignore_ascii_case("production"))
        .unwrap_or(false);

    if !is_production {
        return Ok(());
    }

    const WEBHOOK_SECRETS: &[&str] = &[
        "GITHUB_WEBHOOK_SECRET",
        "SLACK_SIGNING_SECRET",
        "LINEAR_WEBHOOK_SECRET",
        "JIRA_WEBHOOK_SECRET",
    ];

    let mut errors: Vec<String> = Vec::new();

    for name in WEBHOOK_SECRETS {
        match std::env::var(name) {
            Ok(value) if !value.trim().is_empty() && value.trim() != DEV_PLACEHOLDER => {}
            Ok(value) if value.trim() == DEV_PLACEHOLDER => {
                errors.push(format!(
                    "{name} is set to the dev-placeholder value — set a real secret for production"
                ));
            }
            _ => {
                errors.push(format!("{name} must be set in production"));
            }
        }
    }

    if !errors.is_empty() {
        return Err(anyhow::anyhow!(
            "Production secret validation failed:\n  {}",
            errors.join("\n  ")
        ));
    }

    tracing::info!("production secret validation passed");
    Ok(())
}

/// Parse `TRUSTED_PROXY_CIDRS` env var (comma-separated CIDR strings) into
/// `(network_address, prefix_len)` pairs used by the client-IP helper.
/// Malformed entries are logged and skipped.
fn parse_trusted_proxy_cidrs() -> Vec<(std::net::IpAddr, u8)> {
    let raw = match std::env::var("TRUSTED_PROXY_CIDRS") {
        Ok(v) => v,
        Err(_) => return Vec::new(),
    };

    raw.split(',')
        .filter_map(|entry| {
            let entry = entry.trim();
            if entry.is_empty() {
                return None;
            }
            match entry.parse::<ipnet::IpNet>() {
                Ok(net) => Some((net.network(), net.prefix_len())),
                Err(_) => {
                    tracing::warn!(entry, "TRUSTED_PROXY_CIDRS: ignoring invalid CIDR");
                    None
                }
            }
        })
        .collect()
}

fn workspace_creation_secret_from_env() -> anyhow::Result<String> {
    match std::env::var("WORKSPACE_CREATION_SECRET") {
        Ok(value) if !value.trim().is_empty() => Ok(value),
        _ => Err(anyhow::anyhow!("WORKSPACE_CREATION_SECRET must be set")),
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
            // Not configured: LLM summarisation uses a different provider.
            // This is healthy — the API continues to function normally.
            status: "ok".to_owned(),
            latency_ms: None,
            message: Some("not_configured".to_owned()),
        };
    }
    let base = config
        .llm
        .base_url
        .as_deref()
        .unwrap_or("http://localhost:11434");
    let url = format!("{}/api/tags", base.trim_end_matches('/'));

    // Resolve an optional Bearer token — same source as OllamaProvider uses at
    // runtime, so the health probe authenticates consistently with the provider.
    let api_key = config
        .llm
        .ollama
        .as_ref()
        .and_then(|ollama_cfg| ollama_cfg.resolve_api_key());

    let client = reqwest::Client::new();
    let request = {
        let builder = client.get(&url);
        match api_key.as_deref() {
            Some(key) => builder.bearer_auth(key),
            None => builder,
        }
    };

    let started = std::time::Instant::now();
    let result = tokio::time::timeout(Duration::from_secs(2), request.send()).await;
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
            // Configured and reachable but returning an unexpected status.
            // Downgrade to "warn" so optional LLM unavailability does not
            // mark the entire API as unhealthy.
            status: "warn".to_owned(),
            latency_ms: Some(latency_ms),
            message: Some(format!("HTTP {}", resp.status())),
        },
        Ok(Err(error)) => HealthCheck {
            name: "ollama".to_owned(),
            // Configured but unreachable — "warn" keeps the API operational.
            // Check that the base_url is correct; inside Docker use
            // http://host.docker.internal:11434 instead of localhost.
            status: "warn".to_owned(),
            latency_ms: Some(latency_ms),
            message: Some(format!("unreachable: {error}")),
        },
        Err(_) => HealthCheck {
            name: "ollama".to_owned(),
            status: "warn".to_owned(),
            latency_ms: Some(2000),
            message: Some("unreachable: timeout".to_owned()),
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
    #![allow(clippy::unwrap_used, clippy::expect_used, dead_code, unused_imports)]
    use std::{
        net::SocketAddr,
        sync::Arc,
        time::{SystemTime, UNIX_EPOCH},
    };

    use axum::{
        body::{to_bytes, Body},
        extract::connect_info::ConnectInfo,
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
        std::env::set_var("WORKSPACE_CREATION_SECRET", "test-workspace-create-secret");
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
            llm_provider: Arc::new(OllamaProvider::new(
                "http://127.0.0.1:9",
                "test-llm",
                1,
                None,
            )),
            config: Arc::new(config),
            github_webhook_secret: "test-secret".to_owned(),
            trusted_proxy_cidrs: Arc::new(Vec::new()),
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
        let is_workspace_create = method == Method::POST && uri == "/v1/workspaces";
        let mut builder = Request::builder()
            .method(method)
            .uri(uri)
            .header("content-type", "application/json");

        if is_workspace_create {
            if let Ok(secret) = std::env::var("WORKSPACE_CREATION_SECRET") {
                builder = builder
                    .header("x-admin-token", secret)
                    .header("x-forwarded-for", "127.0.0.1");
            }
        }

        if let Some(api_key) = api_key {
            builder = builder.header("x-api-key", api_key);
        }

        match builder.body(Body::from(body.to_string())) {
            Ok(mut request) => {
                if is_workspace_create {
                    request
                        .extensions_mut()
                        .insert(ConnectInfo(SocketAddr::from(([127, 0, 0, 1], 12345))));
                }
                request
            }
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
}
