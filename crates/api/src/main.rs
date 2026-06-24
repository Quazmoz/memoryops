use std::{net::SocketAddr, sync::Arc, time::Duration};

use anyhow::anyhow;
use axum::{
    extract::State, http::StatusCode, middleware as axum_middleware, routing::get, Json, Router,
};
use chrono::Utc;
use common::{
    build_embedding_provider, build_llm_provider, config::AppConfig, db::connect_pool,
    telemetry::init_telemetry, AppState,
};
use qdrant_client::Qdrant;
use retrieval::retrieval_router;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
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
    let app_secret_key = crate::security::app_secret_key_from_env()
        .map_err(|_| anyhow!("APP_SECRET_KEY is missing or invalid -- cannot start"))?;
    crate::security::validate_secret_key(app_secret_key.as_str())
        .map_err(|_| anyhow!("APP_SECRET_KEY is missing or invalid -- cannot start"))?;
    if handlers::workspaces::workspace_creation_enabled_from_env() {
        workspace_creation_secret_from_env()?;
    } else {
        tracing::warn!("workspace creation endpoint disabled by WORKSPACE_CREATION_ENABLED=false");
    }
    validate_production_secrets()?;
    log_audit_startup_config();
    let state = build_state(config.clone(), app_secret_key).await?;
    processor::start_workers(state.clone()).await?;
    tokio::spawn(processor::scheduler::run_scheduler(state.clone()));
    tokio::spawn(processor::scheduler::run_audit_outbox_drainer(
        state.clone(),
    ));

    let address = format!("{}:{}", config.server.host, config.server.port);
    let listener = tokio::net::TcpListener::bind(&address).await?;

    tracing::info!(%address, "starting MemoryOps API");
    axum::serve(
        listener,
        router(state).into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown_signal())
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
        // request_context runs just inside request_id so it sees the resolved
        // x-request-id header; request_id is the outermost layer.
        .layer(axum_middleware::from_fn_with_state(
            state.clone(),
            middleware::request_context::request_context,
        ))
        .layer(axum_middleware::from_fn(middleware::request_id::request_id))
        .with_state(state)
}

async fn build_state(
    config: AppConfig,
    app_secret_key: zeroize::Zeroizing<String>,
) -> anyhow::Result<AppState> {
    let database_url =
        std::env::var("DATABASE_URL").map_err(|_| anyhow::anyhow!("DATABASE_URL not set"))?;
    let redis_url = std::env::var("REDIS_URL").map_err(|_| anyhow::anyhow!("REDIS_URL not set"))?;
    let qdrant_url =
        std::env::var("QDRANT_URL").map_err(|_| anyhow::anyhow!("QDRANT_URL not set"))?;
    let trusted_proxy_cidrs = Arc::new(parse_trusted_proxy_cidrs());

    let db = connect_pool(&database_url, &config.database).await?;
    if std::env::var("SKIP_MIGRATIONS").unwrap_or_default() != "true" {
        tracing::info!("running database migrations...");
        common::db::run_migrations(&db)
            .await
            .map_err(|error| anyhow::anyhow!("failed to run database migrations: {error}"))?;
    } else {
        tracing::info!("bypassing database migrations on startup due to SKIP_MIGRATIONS=true");
    }
    ensure_tool_secret_configuration(&db).await?;
    let redis = {
        let cfg = deadpool_redis::Config::from_url(&redis_url);
        cfg.create_pool(Some(deadpool_redis::Runtime::Tokio1))
            .map_err(|error| anyhow::anyhow!("failed to create Redis pool: {error}"))?
    };
    let mut qdrant_config = qdrant_client::config::QdrantConfig::from_url(&qdrant_url);
    qdrant_config.check_compatibility = false;
    let qdrant = Qdrant::new(qdrant_config)?;

    let embedding_provider = build_embedding_provider(&config);
    let llm_provider = build_llm_provider(&config);

    Ok(AppState {
        db,
        redis,
        qdrant,
        processor_semaphore: Arc::new(Semaphore::new(
            config.processor.fast_path_concurrency.max(1),
        )),
        embedding_provider,
        llm_provider,
        config: Arc::new(config),
        app_secret_key: Arc::new(app_secret_key),
        trusted_proxy_cidrs,
    })
}

async fn ensure_tool_secret_configuration(db: &sqlx::PgPool) -> anyhow::Result<()> {
    let tools_table = sqlx::query_scalar::<_, Option<String>>(
        "SELECT to_regclass('public.workspace_tools')::TEXT",
    )
    .fetch_one(db)
    .await?;
    if tools_table.is_none() {
        return Ok(());
    }

    let has_encrypted_tool = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM workspace_tools WHERE auth_secret_enc IS NOT NULL)",
    )
    .fetch_one(db)
    .await?;
    if has_encrypted_tool
        && std::env::var("APP_SECRET_KEY")
            .map(|value| value.trim().is_empty())
            .unwrap_or(true)
    {
        return Err(anyhow::anyhow!(
            "APP_SECRET_KEY must be set because workspace_tools contains encrypted auth secrets"
        ));
    }

    Ok(())
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

    let mut errors: Vec<String> = Vec::new();

    for name in ["APP_SECRET_KEY"] {
        match std::env::var(name) {
            Ok(value) if !value.trim().is_empty() && value.trim() != DEV_PLACEHOLDER => {}
            Ok(value) if value.trim() == DEV_PLACEHOLDER => {
                errors.push(format!(
                    "{name} is set to the dev-placeholder value; set a real secret for production"
                ));
            }
            _ => errors.push(format!("{name} must be set in production")),
        }
    }

    if handlers::workspaces::workspace_creation_enabled_from_env() {
        match std::env::var("WORKSPACE_CREATION_SECRET") {
            Ok(value) if !value.trim().is_empty() && value.trim() != DEV_PLACEHOLDER => {}
            Ok(value) if value.trim() == DEV_PLACEHOLDER => {
                errors.push(
                    "WORKSPACE_CREATION_SECRET is set to the dev-placeholder value; set a real secret for production or disable workspace creation".to_owned(),
                );
            }
            _ => errors.push(
                "WORKSPACE_CREATION_SECRET must be set in production when workspace creation is enabled".to_owned(),
            ),
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

/// Surface the audit subsystem's effective security posture at startup, so an
/// operator sees — in the boot logs, before any traffic — whether the
/// tamper-evident hash chain is keyed by a dedicated secret and what the
/// retention policy is. None of these are fatal; they are deployment hygiene.
fn log_audit_startup_config() {
    use common::audit::{
        audit_key_source, parse_audit_retention_days, AuditKeySource, AuditRetentionPolicy,
    };

    match audit_key_source() {
        AuditKeySource::Dedicated => {
            tracing::info!("audit hash chain enabled via dedicated AUDIT_SIGNING_KEY");
        }
        AuditKeySource::AppSecretFallback => {
            tracing::warn!(
                "AUDIT_SIGNING_KEY is not set; the audit tamper-evident hash chain is keyed by \
                 APP_SECRET_KEY. Rotating APP_SECRET_KEY will break verification of existing audit \
                 rows. Set a dedicated AUDIT_SIGNING_KEY for production."
            );
        }
        AuditKeySource::Disabled => {
            tracing::warn!(
                "Neither AUDIT_SIGNING_KEY nor APP_SECRET_KEY is set; the audit hash chain is \
                 DISABLED and rows are written without tamper-evidence. Set AUDIT_SIGNING_KEY for \
                 production."
            );
        }
    }

    match parse_audit_retention_days(std::env::var("AUDIT_RETENTION_DAYS").ok().as_deref()) {
        AuditRetentionPolicy::Disabled => {
            tracing::info!(
                "AUDIT_RETENTION_DAYS unset; audit history is retained indefinitely (no pruning)"
            );
        }
        AuditRetentionPolicy::Days(days) => {
            tracing::info!(
                retention_days = days,
                "audit retention policy active: rows older than {days} days are pruned daily"
            );
        }
        AuditRetentionPolicy::Invalid(raw) => {
            tracing::warn!(
                value = %raw,
                "AUDIT_RETENTION_DAYS is invalid (must be an integer in 1..=36500); ignoring it \
                 and retaining audit history indefinitely"
            );
        }
    }
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

async fn shutdown_signal() {
    let ctrl_c = async {
        if let Err(error) = tokio::signal::ctrl_c().await {
            tracing::error!(error = ?error, "failed to install Ctrl+C shutdown handler");
        }
    };

    #[cfg(unix)]
    let terminate = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut signal) => {
                signal.recv().await;
            }
            Err(error) => {
                tracing::error!(error = ?error, "failed to install SIGTERM shutdown handler")
            }
        }
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }

    tracing::info!("shutdown signal received; draining HTTP server");
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

    let mut qdrant_config = qdrant_client::config::QdrantConfig::from_url(&qdrant_url);
    qdrant_config.check_compatibility = false;
    let Ok(client) = Qdrant::new(qdrant_config) else {
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
        let mut qdrant_config = qdrant_client::config::QdrantConfig::from_url(&qdrant_url);
        qdrant_config.check_compatibility = false;
        let qdrant = match Qdrant::new(qdrant_config) {
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
            app_secret_key: Arc::new(zeroize::Zeroizing::new(
                "test-secret-key-for-unit-tests".to_owned(),
            )),
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
        let (plaintext, prefix) = match security::generate_api_key(workspace_id) {
            Ok(generated) => generated,
            Err(error) => panic!("test key should be generated: {error}"),
        };
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
            VALUES ($1, $2, $3, $4, $5, 3, $6, $7)
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
    async fn list_workspaces_returns_only_current_workspace(pool: PgPool) {
        let app = router(test_state(pool.clone()).await);
        let workspace_id = insert_workspace(&pool).await;
        let other_workspace_id = insert_workspace(&pool).await;
        let api_key = insert_api_key(&pool, workspace_id, false).await;

        let response = match app
            .oneshot(request(
                Method::GET,
                "/v1/workspaces".to_owned(),
                Some(&api_key),
                json!(null),
            ))
            .await
        {
            Ok(response) => response,
            Err(error) => panic!("workspace list should respond: {error}"),
        };

        assert_eq!(response.status(), StatusCode::OK);
        let body = response_json(response).await;
        let workspaces = match body.get("workspaces").and_then(Value::as_array) {
            Some(workspaces) => workspaces,
            None => panic!("workspace list response should include workspaces array"),
        };
        assert_eq!(workspaces.len(), 1);
        assert_eq!(
            workspaces[0].get("id").and_then(Value::as_str),
            Some(workspace_id.to_string().as_str())
        );
        assert_eq!(
            workspaces[0].get("name").and_then(Value::as_str),
            Some(format!("workspace-{workspace_id}").as_str())
        );
        assert!(workspaces[0]
            .get("created_at")
            .and_then(Value::as_str)
            .is_some());
        assert!(workspaces
            .iter()
            .all(|workspace| workspace.get("id").and_then(Value::as_str)
                != Some(other_workspace_id.to_string().as_str())));
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn list_workspaces_requires_api_key(pool: PgPool) {
        let app = router(test_state(pool).await);
        let response = match app
            .oneshot(request(
                Method::GET,
                "/v1/workspaces".to_owned(),
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
    async fn bulk_memory_operations(pool: PgPool) {
        let app = router(test_state(pool.clone()).await);
        let workspace_id = insert_workspace(&pool).await;
        let api_key = insert_api_key(&pool, workspace_id, false).await;

        let id1 = insert_memory(&pool, workspace_id, "Memory 1").await;
        let id2 = insert_memory(&pool, workspace_id, "Memory 2").await;

        // 1. Test duplicate IDs requested count
        let response = match app
            .clone()
            .oneshot(request(
                Method::POST,
                "/v1/memory/bulk".to_owned(),
                Some(&api_key),
                json!({
                    "ids": [id1, id1, id2],
                    "action": "pin"
                }),
            ))
            .await
        {
            Ok(response) => response,
            Err(error) => panic!("bulk pin request should respond: {error}"),
        };

        assert_eq!(response.status(), StatusCode::OK);
        let body = response_json(response).await;
        assert_eq!(body["requested"], json!(3));
        assert_eq!(body["affected"], json!(2));
        assert_eq!(body["affected_ids"], json!([id1, id2]));

        // 2. Test validation: empty IDs
        let response = match app
            .clone()
            .oneshot(request(
                Method::POST,
                "/v1/memory/bulk".to_owned(),
                Some(&api_key),
                json!({
                    "ids": [],
                    "action": "pin"
                }),
            ))
            .await
        {
            Ok(response) => response,
            Err(error) => panic!("empty bulk pin request should respond: {error}"),
        };
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);

        // 3. Test validation: >100 IDs
        let mut many_ids = Vec::new();
        for _ in 0..101 {
            many_ids.push(Uuid::now_v7());
        }
        let response = match app
            .clone()
            .oneshot(request(
                Method::POST,
                "/v1/memory/bulk".to_owned(),
                Some(&api_key),
                json!({
                    "ids": many_ids,
                    "action": "pin"
                }),
            ))
            .await
        {
            Ok(response) => response,
            Err(error) => panic!("overlimit bulk pin request should respond: {error}"),
        };
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);

        // 4. Test already-deleted ID behavior
        // Delete id1 first
        let _deleted = sqlx::query("UPDATE memory_units SET deleted_at = now() WHERE id = $1")
            .bind(id1)
            .execute(&pool)
            .await
            .unwrap();

        // Now try bulk deleting them
        let response = match app
            .clone()
            .oneshot(request(
                Method::POST,
                "/v1/memory/bulk".to_owned(),
                Some(&api_key),
                json!({
                    "ids": [id1, id2],
                    "action": "delete"
                }),
            ))
            .await
        {
            Ok(response) => response,
            Err(error) => panic!("bulk delete request should respond: {error}"),
        };
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    /// Creating an API key must produce a reliable audit row that carries the
    /// authenticated actor + API key context, and the audit list must filter by
    /// action. Exercises the reliable write path, request-context middleware, and
    /// the action filter together.
    #[sqlx::test(migrations = "../../migrations")]
    async fn key_creation_is_audited_with_actor_context(pool: PgPool) {
        let app = router(test_state(pool.clone()).await);
        let workspace_id = insert_workspace(&pool).await;
        let api_key = insert_api_key(&pool, workspace_id, false).await;

        let create = app
            .clone()
            .oneshot(request(
                Method::POST,
                format!("/v1/workspaces/{workspace_id}/keys"),
                Some(&api_key),
                json!({ "name": "ci-key" }),
            ))
            .await
            .expect("create key responds");
        assert_eq!(create.status(), StatusCode::OK);

        let response = app
            .oneshot(request(
                Method::GET,
                format!("/v1/workspaces/{workspace_id}/audit?action=key_created"),
                Some(&api_key),
                json!(null),
            ))
            .await
            .expect("audit list responds");
        assert_eq!(response.status(), StatusCode::OK);
        let body = response_json(response).await;
        let items = body
            .get("items")
            .and_then(Value::as_array)
            .expect("items array");
        assert!(!items.is_empty(), "key_created audit row should exist");
        let entry = &items[0];
        assert_eq!(entry["action"], json!("key_created"));
        assert!(entry["actor"]
            .as_str()
            .is_some_and(|actor| actor.starts_with("api_key:")));
        assert!(
            entry.get("api_key_id").and_then(Value::as_str).is_some(),
            "audit row should record api_key_id context"
        );
    }

    /// Audit payloads must never persist secret values, even when a handler is
    /// handed one. Updating workspace config with a secret-bearing field and
    /// reading it back through the audit API must show `[REDACTED]`.
    #[sqlx::test(migrations = "../../migrations")]
    async fn audit_redacts_secrets_in_config_diff(pool: PgPool) {
        let app = router(test_state(pool.clone()).await);
        let workspace_id = insert_workspace(&pool).await;
        let api_key = insert_api_key(&pool, workspace_id, false).await;

        let patch = app
            .clone()
            .oneshot(request(
                Method::PATCH,
                format!("/v1/workspaces/{workspace_id}/config"),
                Some(&api_key),
                json!({ "llm_api_key_env": "OPENAI_API_KEY" }),
            ))
            .await
            .expect("config patch responds");
        assert_eq!(patch.status(), StatusCode::OK);

        let response = app
            .oneshot(request(
                Method::GET,
                format!("/v1/workspaces/{workspace_id}/audit?category=workspace"),
                Some(&api_key),
                json!(null),
            ))
            .await
            .expect("audit list responds");
        assert_eq!(response.status(), StatusCode::OK);
        let raw = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("audit body");
        let text = String::from_utf8_lossy(&raw);
        // The env-var *name* contains "key", so the policy redacts it: the audit
        // log proves a key-related field changed without storing its value.
        assert!(
            text.contains("[REDACTED]"),
            "expected a redacted key-like field in audit output: {text}"
        );
    }

    // ───────────────────────────── Audit API coverage ─────────────────────────

    struct AuditRowSpec<'a> {
        actor: &'a str,
        action: &'a str,
        target_type: &'a str,
        category: Option<&'a str>,
        severity: &'a str,
        success: bool,
        metadata: Option<Value>,
        diff: Option<Value>,
    }

    /// Insert a raw `audit_log` row with NULL seq/hash, mirroring a row written
    /// before audit hardening. Lets the read/export/redaction/filter paths be
    /// exercised deterministically and independently of the write path.
    async fn insert_audit_row(pool: &PgPool, workspace_id: Uuid, spec: AuditRowSpec<'_>) -> Uuid {
        let id = Uuid::now_v7();
        let result = sqlx::query(
            r#"
            INSERT INTO audit_log
                (id, workspace_id, actor, action, target_id, target_type,
                 category, severity, success, metadata, diff)
            VALUES ($1, $2, $3, $4::audit_action, $5, $6, $7, $8, $9, $10, $11)
            "#,
        )
        .bind(id)
        .bind(workspace_id)
        .bind(spec.actor)
        .bind(spec.action)
        .bind(id)
        .bind(spec.target_type)
        .bind(spec.category)
        .bind(spec.severity)
        .bind(spec.success)
        .bind(spec.metadata)
        .bind(spec.diff)
        .execute(pool)
        .await;
        if let Err(error) = result {
            panic!("audit row insert should succeed: {error}");
        }
        id
    }

    /// Three rows: two `api_key`/critical (one success) and one `security`/warning failure.
    async fn seed_audit_rows(pool: &PgPool, workspace_id: Uuid) {
        insert_audit_row(
            pool,
            workspace_id,
            AuditRowSpec {
                actor: "api_key:alpha",
                action: "key_created",
                target_type: "api_key",
                category: Some("api_key"),
                severity: "critical",
                success: true,
                metadata: Some(json!({ "name": "k1" })),
                diff: None,
            },
        )
        .await;
        insert_audit_row(
            pool,
            workspace_id,
            AuditRowSpec {
                actor: "api_key:alpha",
                action: "key_revoked",
                target_type: "api_key",
                category: Some("api_key"),
                severity: "critical",
                success: true,
                metadata: None,
                diff: None,
            },
        )
        .await;
        insert_audit_row(
            pool,
            workspace_id,
            AuditRowSpec {
                actor: "system",
                action: "auth_failed",
                target_type: "api_key",
                category: Some("security"),
                severity: "warning",
                success: false,
                metadata: Some(json!({ "note": "bad key" })),
                diff: None,
            },
        )
        .await;
    }

    fn items_of(body: &Value) -> &Vec<Value> {
        body.get("items")
            .and_then(Value::as_array)
            .expect("audit response should contain items array")
    }

    async fn audit_list(
        app: &axum::Router,
        api_key: &str,
        workspace_id: Uuid,
        query: &str,
    ) -> Value {
        let uri = format!("/v1/workspaces/{workspace_id}/audit{query}");
        let response = app
            .clone()
            .oneshot(request(Method::GET, uri, Some(api_key), json!(null)))
            .await
            .expect("audit list should respond");
        assert_eq!(response.status(), StatusCode::OK);
        response_json(response).await
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn audit_list_supports_filters(pool: PgPool) {
        let app = router(test_state(pool.clone()).await);
        let workspace_id = insert_workspace(&pool).await;
        let api_key = insert_api_key(&pool, workspace_id, false).await;
        seed_audit_rows(&pool, workspace_id).await;

        assert_eq!(
            audit_list(&app, &api_key, workspace_id, "").await["items"]
                .as_array()
                .map(Vec::len)
                .unwrap_or_default(),
            3
        );

        let by_action = audit_list(&app, &api_key, workspace_id, "?action=key_created").await;
        assert_eq!(items_of(&by_action).len(), 1);
        assert_eq!(items_of(&by_action)[0]["action"], json!("key_created"));

        let by_actions = audit_list(
            &app,
            &api_key,
            workspace_id,
            "?actions=key_created,key_revoked",
        )
        .await;
        assert_eq!(items_of(&by_actions).len(), 2);

        let by_category = audit_list(&app, &api_key, workspace_id, "?category=api_key").await;
        assert_eq!(items_of(&by_category).len(), 2);

        let by_severity = audit_list(&app, &api_key, workspace_id, "?severity=warning").await;
        assert_eq!(items_of(&by_severity).len(), 1);
        assert_eq!(items_of(&by_severity)[0]["action"], json!("auth_failed"));

        let failures = audit_list(&app, &api_key, workspace_id, "?success=false").await;
        assert_eq!(items_of(&failures).len(), 1);

        let by_actor = audit_list(&app, &api_key, workspace_id, "?actor=api_key:alpha").await;
        assert_eq!(items_of(&by_actor).len(), 2);
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn audit_get_entry_and_missing(pool: PgPool) {
        let app = router(test_state(pool.clone()).await);
        let workspace_id = insert_workspace(&pool).await;
        let api_key = insert_api_key(&pool, workspace_id, false).await;
        let audit_id = insert_audit_row(
            &pool,
            workspace_id,
            AuditRowSpec {
                actor: "api_key:alpha",
                action: "key_created",
                target_type: "api_key",
                category: Some("api_key"),
                severity: "critical",
                success: true,
                metadata: Some(json!({ "name": "k1" })),
                diff: None,
            },
        )
        .await;

        let response = app
            .clone()
            .oneshot(request(
                Method::GET,
                format!("/v1/workspaces/{workspace_id}/audit/{audit_id}"),
                Some(&api_key),
                json!(null),
            ))
            .await
            .expect("get entry should respond");
        assert_eq!(response.status(), StatusCode::OK);
        let body = response_json(response).await;
        assert_eq!(body["id"], json!(audit_id.to_string()));
        assert_eq!(body["action"], json!("key_created"));

        let missing = Uuid::now_v7();
        let response = app
            .oneshot(request(
                Method::GET,
                format!("/v1/workspaces/{workspace_id}/audit/{missing}"),
                Some(&api_key),
                json!(null),
            ))
            .await
            .expect("missing entry should respond");
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn audit_actions_endpoint_lists_metadata(pool: PgPool) {
        let app = router(test_state(pool.clone()).await);
        let workspace_id = insert_workspace(&pool).await;
        let api_key = insert_api_key(&pool, workspace_id, false).await;

        let response = app
            .oneshot(request(
                Method::GET,
                format!("/v1/workspaces/{workspace_id}/audit/actions"),
                Some(&api_key),
                json!(null),
            ))
            .await
            .expect("actions should respond");
        assert_eq!(response.status(), StatusCode::OK);
        let body = response_json(response).await;

        let actions = body
            .get("actions")
            .and_then(Value::as_array)
            .expect("actions array");
        assert!(!actions.is_empty());
        let key_created = actions
            .iter()
            .find(|a| a["name"] == json!("key_created"))
            .expect("key_created should be present");
        assert_eq!(key_created["required"], json!(true));
        let tool_invoked = actions
            .iter()
            .find(|a| a["name"] == json!("tool_invoked"))
            .expect("tool_invoked should be present");
        assert_eq!(tool_invoked["required"], json!(false));

        assert_eq!(
            body.get("severities")
                .and_then(Value::as_array)
                .map(Vec::len),
            Some(4)
        );
        assert!(body
            .get("categories")
            .and_then(Value::as_array)
            .is_some_and(|c| !c.is_empty()));
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn audit_export_jsonl(pool: PgPool) {
        let app = router(test_state(pool.clone()).await);
        let workspace_id = insert_workspace(&pool).await;
        let api_key = insert_api_key(&pool, workspace_id, false).await;
        seed_audit_rows(&pool, workspace_id).await;

        let response = app
            .oneshot(request(
                Method::GET,
                format!("/v1/workspaces/{workspace_id}/audit/export?format=jsonl"),
                Some(&api_key),
                json!(null),
            ))
            .await
            .expect("jsonl export should respond");
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response
                .headers()
                .get(axum::http::header::CONTENT_TYPE)
                .and_then(|v| v.to_str().ok()),
            Some("application/x-ndjson; charset=utf-8")
        );
        let raw = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("jsonl body");
        let text = String::from_utf8_lossy(&raw);
        let lines: Vec<&str> = text.lines().filter(|l| !l.is_empty()).collect();
        assert_eq!(lines.len(), 3, "one ndjson line per seeded row");
        for line in &lines {
            serde_json::from_str::<Value>(line).expect("each export line is valid JSON");
        }
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn audit_export_csv(pool: PgPool) {
        let app = router(test_state(pool.clone()).await);
        let workspace_id = insert_workspace(&pool).await;
        let api_key = insert_api_key(&pool, workspace_id, false).await;
        seed_audit_rows(&pool, workspace_id).await;

        let response = app
            .oneshot(request(
                Method::GET,
                format!("/v1/workspaces/{workspace_id}/audit/export?format=csv"),
                Some(&api_key),
                json!(null),
            ))
            .await
            .expect("csv export should respond");
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response
                .headers()
                .get(axum::http::header::CONTENT_TYPE)
                .and_then(|v| v.to_str().ok()),
            Some("text/csv; charset=utf-8")
        );
        let raw = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("csv body");
        let text = String::from_utf8_lossy(&raw);
        let lines: Vec<&str> = text.lines().filter(|l| !l.is_empty()).collect();
        assert!(
            lines[0].starts_with("occurred_at,id,actor"),
            "csv header: {}",
            lines[0]
        );
        assert_eq!(lines.len(), 4, "csv header + 3 seeded rows");
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn audit_export_rejects_unknown_format(pool: PgPool) {
        let app = router(test_state(pool.clone()).await);
        let workspace_id = insert_workspace(&pool).await;
        let api_key = insert_api_key(&pool, workspace_id, false).await;

        let response = app
            .oneshot(request(
                Method::GET,
                format!("/v1/workspaces/{workspace_id}/audit/export?format=xml"),
                Some(&api_key),
                json!(null),
            ))
            .await
            .expect("bad format should respond");
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn audit_verify_endpoint_reports_chain_state(pool: PgPool) {
        let app = router(test_state(pool.clone()).await);
        let workspace_id = insert_workspace(&pool).await;
        let api_key = insert_api_key(&pool, workspace_id, false).await;

        // Write three properly-chained rows through the reliable path.
        for i in 0..3 {
            let event = common::audit::AuditEvent::new(
                workspace_id,
                common::models::AuditAction::KeyCreated,
                Uuid::now_v7(),
                "api_key",
            )
            .actor_string("api_key:alpha")
            .metadata(json!({ "i": i }));
            common::audit::write_audit(&pool, &event)
                .await
                .expect("write chained audit row");
        }

        let response = app
            .oneshot(request(
                Method::POST,
                format!("/v1/workspaces/{workspace_id}/audit/verify"),
                Some(&api_key),
                json!(null),
            ))
            .await
            .expect("verify should respond");
        assert_eq!(response.status(), StatusCode::OK);
        let body = response_json(response).await;
        // The endpoint always answers; chain checking only runs when a signing
        // key is configured (CI's unit-test job runs without one).
        assert!(body.get("enabled").and_then(Value::as_bool).is_some());
        if body["enabled"] == json!(true) {
            assert_eq!(body["verified"], json!(true), "fresh chain must verify");
            assert_eq!(body["checked"], json!(3));
        }
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn audit_is_workspace_isolated(pool: PgPool) {
        let app = router(test_state(pool.clone()).await);
        let workspace_a = insert_workspace(&pool).await;
        let workspace_b = insert_workspace(&pool).await;
        let key_a = insert_api_key(&pool, workspace_a, false).await;
        seed_audit_rows(&pool, workspace_a).await;
        let b_only = insert_audit_row(
            &pool,
            workspace_b,
            AuditRowSpec {
                actor: "api_key:beta",
                action: "workspace_deleted",
                target_type: "workspace",
                category: Some("workspace"),
                severity: "critical",
                success: true,
                metadata: None,
                diff: None,
            },
        )
        .await;

        // A's key lists A's audit → only A's rows, never B's.
        let body = audit_list(&app, &key_a, workspace_a, "").await;
        let items = items_of(&body);
        assert_eq!(items.len(), 3);
        assert!(items
            .iter()
            .all(|item| item["id"] != json!(b_only.to_string())));

        // A's key cannot read B's row by id (workspace-scoped query → 404).
        let response = app
            .clone()
            .oneshot(request(
                Method::GET,
                format!("/v1/workspaces/{workspace_a}/audit/{b_only}"),
                Some(&key_a),
                json!(null),
            ))
            .await
            .expect("cross-workspace read should respond");
        assert_eq!(response.status(), StatusCode::NOT_FOUND);

        // A's key cannot target B's workspace path at all (require_workspace → 403).
        let response = app
            .oneshot(request(
                Method::GET,
                format!("/v1/workspaces/{workspace_b}/audit"),
                Some(&key_a),
                json!(null),
            ))
            .await
            .expect("cross-workspace path should respond");
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn audit_read_and_export_redact_legacy_rows(pool: PgPool) {
        let app = router(test_state(pool.clone()).await);
        let workspace_id = insert_workspace(&pool).await;
        let api_key = insert_api_key(&pool, workspace_id, false).await;

        // Legacy row (NULL seq/hash) whose payloads carry secret-keyed fields that
        // were never redacted at write time. Redaction on read/export must catch it.
        let secret = "whsec_legacy_unredacted_value";
        let audit_id = insert_audit_row(
            &pool,
            workspace_id,
            AuditRowSpec {
                actor: "system",
                action: "integration_updated",
                target_type: "integration",
                category: Some("integration"),
                severity: "notice",
                success: true,
                metadata: Some(
                    json!({ "webhook_secret": secret, "endpoint": "https://example.com" }),
                ),
                diff: Some(json!({ "api_key": secret })),
            },
        )
        .await;

        let assert_no_secret = |raw: &[u8], where_: &str| {
            let text = String::from_utf8_lossy(raw);
            assert!(
                !text.contains(secret),
                "{where_} leaked legacy secret: {text}"
            );
            assert!(
                text.contains("[REDACTED]"),
                "{where_} should mark redaction"
            );
        };

        // Single read: secret masked, non-sensitive field preserved.
        let response = app
            .clone()
            .oneshot(request(
                Method::GET,
                format!("/v1/workspaces/{workspace_id}/audit/{audit_id}"),
                Some(&api_key),
                json!(null),
            ))
            .await
            .expect("get entry should respond");
        let raw = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("entry body");
        assert_no_secret(&raw, "single read");
        assert!(String::from_utf8_lossy(&raw).contains("https://example.com"));

        // List.
        let response = app
            .clone()
            .oneshot(request(
                Method::GET,
                format!("/v1/workspaces/{workspace_id}/audit"),
                Some(&api_key),
                json!(null),
            ))
            .await
            .expect("list should respond");
        let raw = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("list body");
        assert_no_secret(&raw, "list");

        // JSONL export.
        let response = app
            .clone()
            .oneshot(request(
                Method::GET,
                format!("/v1/workspaces/{workspace_id}/audit/export?format=jsonl"),
                Some(&api_key),
                json!(null),
            ))
            .await
            .expect("jsonl export should respond");
        let raw = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("jsonl body");
        assert_no_secret(&raw, "jsonl export");

        // CSV export.
        let response = app
            .oneshot(request(
                Method::GET,
                format!("/v1/workspaces/{workspace_id}/audit/export?format=csv"),
                Some(&api_key),
                json!(null),
            ))
            .await
            .expect("csv export should respond");
        let raw = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("csv body");
        assert_no_secret(&raw, "csv export");
    }
}
