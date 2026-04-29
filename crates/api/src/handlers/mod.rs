use axum::{extract::DefaultBodyLimit, routing::get, Router};
use common::{auth::AuthContext, error::AppResult, AppError, AppState};
use uuid::Uuid;

pub mod audit;
pub mod contradictions;
pub mod export;
pub mod integrations;
pub mod keys;
pub mod metrics;
pub mod skills;
pub mod tags;
pub mod workspaces;

pub fn bootstrap_router() -> Router<AppState> {
    Router::new().route(
        "/v1/workspaces",
        axum::routing::post(workspaces::create_workspace),
    )
}

pub fn protected_router() -> Router<AppState> {
    Router::new()
        .route("/v1/workspaces/{id}", get(workspaces::get_workspace))
        .route("/v1/workspaces/{id}/stats", get(workspaces::get_stats))
        .route("/v1/workspaces/{id}/metrics", get(metrics::get_metrics))
        .route(
            "/v1/workspaces/{id}/stats/history",
            get(workspaces::get_stats_history),
        )
        .route("/v1/workspaces/{id}/export", get(export::export_workspace))
        .route(
            "/v1/workspaces/{id}/import",
            axum::routing::post(workspaces::import_memories)
                .layer(DefaultBodyLimit::max(workspaces::MAX_IMPORT_BODY_BYTES)),
        )
        .route("/v1/workspaces/{id}/tags", get(tags::list_tags))
        .route(
            "/v1/workspaces/{id}/config",
            axum::routing::patch(workspaces::update_workspace_config),
        )
        .route(
            "/v1/workspaces/{id}/promote",
            axum::routing::post(workspaces::promote),
        )
        .route(
            "/v1/workspaces/{id}/keys",
            axum::routing::post(keys::create_key).get(keys::list_keys),
        )
        .route(
            "/v1/workspaces/{id}/keys/{key_id}",
            axum::routing::delete(keys::revoke_key),
        )
        .route("/v1/workspaces/{id}/audit", get(audit::list_audit))
        .route(
            "/v1/workspaces/{id}/contradictions",
            get(contradictions::list_contradictions),
        )
        .route(
            "/v1/workspaces/{id}/contradictions/count",
            get(contradictions::get_contradiction_count),
        )
        .route(
            "/v1/workspaces/{id}/contradictions/{flag_id}/resolve",
            axum::routing::post(contradictions::resolve_contradiction),
        )
        .route(
            "/v1/workspaces/{id}/skills",
            axum::routing::post(skills::create_skill).get(skills::list_skills),
        )
        .route(
            "/v1/workspaces/{id}/skills/{name}",
            get(skills::get_skill)
                .patch(skills::update_skill)
                .delete(skills::delete_skill),
        )
        .route(
            "/v1/workspaces/{id}/skills/{name}/secret",
            get(skills::get_skill_secret),
        )
        .route(
            "/v1/workspaces/{id}/integrations",
            axum::routing::post(integrations::create_integration)
                .get(integrations::list_integrations),
        )
        .route(
            "/v1/workspaces/{id}/integrations/{source}",
            axum::routing::delete(integrations::delete_integration),
        )
        .route("/v1/workspaces/{id}/dlq", get(integrations::list_dlq))
        .route(
            "/v1/workspaces/{id}/dlq/{job_id}/retry",
            axum::routing::post(integrations::retry_dlq),
        )
        .route(
            "/v1/workspaces/{id}/dlq/{job_id}",
            axum::routing::delete(integrations::delete_dlq),
        )
}

pub(crate) fn require_workspace(auth: &AuthContext, workspace_id: Uuid) -> AppResult<()> {
    if auth.workspace_id == workspace_id {
        Ok(())
    } else {
        Err(AppError::Forbidden)
    }
}
