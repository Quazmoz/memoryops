pub mod access;
pub mod dto;
pub mod handlers;
pub mod promotion;
pub mod search;
pub mod services;
pub mod store;

use axum::{
    routing::{get, post},
    Router,
};
use common::AppState;
use tower_http::trace::TraceLayer;

pub use dto::{
    ListQuery, ListResponse, MemoryResult, MemoryUnitDto, SearchRequest, SearchResponse,
};
pub use promotion::decay::run_decay_pass;

pub fn retrieval_router() -> Router<AppState> {
    Router::new()
        .route("/v1/memory/search", post(handlers::search::handle_search))
        .route("/v1/retrieve", post(handlers::retrieve::handle_retrieve))
        .route(
            "/v1/retrieve/trace/{query_id}",
            get(handlers::retrieve::handle_trace_get),
        )
        .route(
            "/v1/memory",
            get(handlers::list::handle_list).post(handlers::create::handle_create),
        )
        .route("/v1/memory/bulk", post(handlers::lifecycle::handle_bulk))
        .route("/v1/memory/merge", post(handlers::lifecycle::handle_merge))
        .route(
            "/v1/memory/{id}/restore",
            post(handlers::lifecycle::handle_restore),
        )
        .route(
            "/v1/memory/{id}/promote",
            post(handlers::lifecycle::handle_promote),
        )
        .route(
            "/v1/memory/{id}/publish",
            post(handlers::lifecycle::handle_publish),
        )
        .route(
            "/v1/memory/{id}/history",
            get(handlers::lifecycle::handle_history),
        )
        .route(
            "/v1/memory/{id}/provenance",
            get(handlers::provenance::handle_provenance),
        )
        .route(
            "/v1/memory/{id}/feedback",
            get(handlers::feedback::handle_list_feedback)
                .post(handlers::feedback::handle_submit_feedback),
        )
        .route(
            "/v1/memory/{id}",
            get(handlers::get::handle_get)
                .patch(handlers::update::handle_update)
                .delete(handlers::lifecycle::handle_delete),
        )
        .layer(TraceLayer::new_for_http())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn router_builds() {
        let _router = retrieval_router();
    }
}
