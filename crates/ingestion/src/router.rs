use axum::{routing::post, Router};
use common::AppState;
use tower_http::trace::TraceLayer;

use crate::github;

pub fn ingestion_router() -> Router<AppState> {
    Router::new()
        .route(
            "/v1/ingest/github",
            post(github::handler::handle_github_webhook),
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
