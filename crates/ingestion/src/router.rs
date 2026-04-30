use axum::{routing::post, Router};
use common::AppState;
use tower_http::trace::TraceLayer;

use crate::{github, jira, linear, observation, slack};

pub fn ingestion_router() -> Router<AppState> {
    Router::new()
        .route(
            "/v1/ingest/github",
            post(github::handler::handle_github_webhook),
        )
        .route(
            "/v1/ingest/slack",
            post(slack::handler::handle_slack_webhook),
        )
        .route(
            "/v1/ingest/linear",
            post(linear::handler::handle_linear_webhook),
        )
        .route("/v1/ingest/jira", post(jira::handler::handle_jira_webhook))
        .layer(TraceLayer::new_for_http())
}

pub fn observation_router() -> Router<AppState> {
    Router::new()
        .route(
            "/v1/ingest/observation",
            post(observation::handler::handle_ingest_observation),
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
