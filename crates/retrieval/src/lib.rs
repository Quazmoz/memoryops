pub mod access;
pub mod dto;
pub mod handlers;
pub mod promotion;
pub mod search;
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
        .route("/v1/memory", get(handlers::list::handle_list))
        .route(
            "/v1/memory/{id}",
            get(handlers::get::handle_get).patch(handlers::update::handle_update),
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
