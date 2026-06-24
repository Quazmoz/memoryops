use axum::{extract::DefaultBodyLimit, routing::get, Router};
use common::{auth::AuthContext, error::AppResult, AppError, AppState};
use uuid::Uuid;

pub mod admin;
pub mod agent_resources;
pub mod agent_skills;
pub mod audit;
pub mod compliance;
pub mod contradictions;
pub mod default_workspace;
pub mod export;
pub mod integration_dlq;
pub mod integration_sync;
pub mod integrations;
pub mod keys;
pub mod metrics;
pub mod tags;
pub mod tools;
pub mod workspaces;

pub fn bootstrap_router() -> Router<AppState> {
    Router::new()
        .route(
            "/v1/workspaces",
            axum::routing::post(workspaces::create_workspace),
        )
        .route(
            "/v1/default-workspace",
            get(default_workspace::get_default_workspace),
        )
        .route("/v1/admin/session", axum::routing::post(admin::login))
}

pub fn protected_router() -> Router<AppState> {
    Router::new()
        // GET only: POST /v1/workspaces stays in bootstrap_router (admin-token
        // auth). Axum merges the two method routers for the shared path.
        .route("/v1/workspaces", get(workspaces::list_workspaces))
        .route("/v1/workspaces/me", get(workspaces::get_current_workspace))
        .route(
            "/v1/workspaces/{id}",
            get(workspaces::get_workspace).delete(workspaces::delete_workspace),
        )
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
            "/v1/workspaces/{id}/audit/actions",
            get(audit::list_audit_actions),
        )
        .route("/v1/workspaces/{id}/audit/export", get(audit::export_audit))
        .route(
            "/v1/workspaces/{id}/audit/verify",
            axum::routing::post(audit::verify_audit),
        )
        .route(
            "/v1/workspaces/{id}/audit/{audit_id}",
            get(audit::get_audit_entry),
        )
        .route(
            "/v1/workspaces/{workspace_id}/forget/user/{user_id}",
            axum::routing::delete(compliance::forget_user_data),
        )
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
            "/v1/workspaces/{id}/contradictions/bulk-dismiss",
            axum::routing::post(contradictions::bulk_dismiss_contradictions),
        )
        .route(
            "/v1/workspaces/{id}/reindex",
            axum::routing::post(workspaces::reindex_workspace),
        )
        .route(
            "/v1/workspaces/{id}/tools",
            axum::routing::post(tools::create_tool).get(tools::list_tools),
        )
        .route(
            "/v1/workspaces/{id}/tools/{name}",
            get(tools::get_tool)
                .patch(tools::update_tool)
                .delete(tools::delete_tool),
        )
        .route(
            "/v1/workspaces/{id}/tools/{name}/secret",
            get(tools::get_tool_secret),
        )
        .route(
            "/v1/workspaces/{id}/tools/{name}/test",
            axum::routing::post(tools::test_tool),
        )
        .route(
            "/v1/workspaces/{id}/tools/{name}/versions",
            get(tools::list_tool_versions),
        )
        .route(
            "/v1/workspaces/{id}/tools/{name}/versions/{version}",
            get(tools::get_tool_version),
        )
        .route(
            "/v1/workspaces/{id}/tools/{name}/versions/{version}/rollback",
            axum::routing::post(tools::rollback_tool_version),
        )
        .route(
            "/v1/workspaces/{id}/tools/{name}/invoke",
            axum::routing::post(tools::invoke_tool),
        )
        .route(
            "/v1/workspaces/{id}/tools/{name}/invocations",
            get(tools::list_tool_invocations),
        )
        .route("/v1/workspaces/{id}/tools/export", get(tools::export_tools))
        .route(
            "/v1/workspaces/{id}/tools/import",
            axum::routing::post(tools::import_tools),
        )
        .route(
            "/v1/agent-resources",
            get(agent_resources::list_agent_resources).post(agent_resources::create_agent_resource),
        )
        .route(
            "/v1/agent-resources/{kind}/{assistant}/{name}",
            get(agent_resources::get_agent_resource)
                .put(agent_resources::update_agent_resource)
                .delete(agent_resources::delete_agent_resource),
        )
        .route(
            "/v1/agent-resources/{kind}/{assistant}/{name}/versions",
            get(agent_resources::list_agent_resource_versions),
        )
        .route(
            "/v1/agent-resources/{kind}/{assistant}/{name}/versions/{version}",
            get(agent_resources::get_agent_resource_version),
        )
        .route(
            "/v1/agent-resources/{kind}/{assistant}/{name}/versions/{version}/rollback",
            axum::routing::post(agent_resources::rollback_agent_resource),
        )
        .route(
            "/v1/agent-skills",
            get(agent_skills::list_agent_skills).post(agent_skills::create_agent_skill),
        )
        .route(
            "/v1/agent-skills/{assistant}/{name}",
            get(agent_skills::get_agent_skill).put(agent_skills::update_agent_skill),
        )
        .route(
            "/v1/agent-skills/{assistant}/{name}/versions",
            get(agent_skills::list_agent_skill_versions),
        )
        .route(
            "/v1/agent-skills/{assistant}/{name}/versions/{version}",
            get(agent_skills::get_agent_skill_version),
        )
        .route(
            "/v1/agent-skills/{assistant}/{name}/versions/{version}/rollback",
            axum::routing::post(agent_skills::rollback_agent_skill_version),
        )
        .route(
            "/v1/workspaces/{id}/integrations",
            axum::routing::post(integrations::create_integration)
                .get(integrations::list_integrations),
        )
        .route(
            "/v1/workspaces/{id}/integrations/{source}/sync",
            axum::routing::post(integrations::start_connector_sync),
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
