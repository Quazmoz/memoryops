use axum::{extract::DefaultBodyLimit, routing::post, Router};
use common::AppState;
use tower_http::trace::TraceLayer;

use crate::{github, jira, linear, observation, slack};

pub fn ingestion_router() -> Router<AppState> {
    Router::new()
        .route(
            "/v1/ingest/github/{workspace_id}",
            post(github::handler::handle_github_webhook),
        )
        .route(
            "/v1/ingest/slack/{workspace_id}",
            post(slack::handler::handle_slack_webhook),
        )
        .route(
            "/v1/ingest/linear/{workspace_id}",
            post(linear::handler::handle_linear_webhook),
        )
        .route(
            "/v1/ingest/jira/{workspace_id}",
            post(jira::handler::handle_jira_webhook),
        )
        .layer(TraceLayer::new_for_http())
}

pub fn observation_router() -> Router<AppState> {
    Router::new()
        .route(
            "/v1/ingest/observation",
            post(observation::handler::handle_ingest_observation).layer(DefaultBodyLimit::max(
                observation::handler::MAX_OBSERVATION_BODY_BYTES,
            )),
        )
        .layer(TraceLayer::new_for_http())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn router_builds() {
        let _router = ingestion_router();
    }
}
