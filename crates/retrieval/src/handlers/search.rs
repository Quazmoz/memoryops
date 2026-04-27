use axum::{extract::State, Extension, Json};
use common::{auth::AuthContext, error::AppResult, AppError, AppState};
use uuid::Uuid;
use validator::Validate;

use crate::{
    access,
    dto::{SearchMode, SearchRequest, SearchResponse, DEFAULT_LIMIT, MAX_LIMIT},
    promotion,
    search::{hybrid, keyword, vector},
};

use super::resolve_workspace_id;

#[axum::debug_handler]
pub async fn handle_search(
    State(state): State<AppState>,
    auth: Option<Extension<AuthContext>>,
    Json(req): Json<SearchRequest>,
) -> AppResult<Json<SearchResponse>> {
    Validate::validate(&req).map_err(|error| AppError::Validation(error.to_string()))?;

    let limit = req.limit.unwrap_or(DEFAULT_LIMIT).min(MAX_LIMIT);
    let auth_context = auth.as_ref().map(|extension| &extension.0);
    let workspace_id = resolve_workspace_id(auth_context, Some(req.workspace_id))?;
    let results = match req.mode {
        SearchMode::Vector => vector::vector_search_results(&state, &req, limit).await?,
        SearchMode::Keyword => keyword::keyword_search(&state, &req, limit).await?,
        SearchMode::Hybrid => hybrid::hybrid_search(&state, &req, limit).await?,
    };

    for result in &results {
        if let Err(error) = access::record_access(&state.redis, result.memory.id).await {
            tracing::warn!(error = ?error, memory_id = %result.memory.id, "failed to record memory access");
        }
    }

    let result_ids = results
        .iter()
        .map(|result| result.memory.id)
        .collect::<Vec<_>>();
    if !result_ids.is_empty() {
        let task_state = state.clone();
        tokio::spawn(async move {
            promotion::check_and_promote(task_state, workspace_id, result_ids).await;
        });
    }

    Ok(Json(SearchResponse {
        total: results.len() as u64,
        results,
        query_id: Uuid::now_v7(),
    }))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn search_request_validation_rejects_empty_query() {
        let workspace_id = Uuid::now_v7();
        let request = match serde_json::from_value::<SearchRequest>(json!({
            "query": "",
            "workspace_id": workspace_id,
            "mode": "hybrid"
        })) {
            Ok(request) => request,
            Err(error) => panic!("request should deserialize before validation: {error}"),
        };

        assert!(Validate::validate(&request).is_err());
    }

    #[test]
    fn search_request_validation_rejects_overlimit() {
        let workspace_id = Uuid::now_v7();
        let request = match serde_json::from_value::<SearchRequest>(json!({
            "query": "memory",
            "workspace_id": workspace_id,
            "mode": "keyword",
            "limit": 101
        })) {
            Ok(request) => request,
            Err(error) => panic!("request should deserialize before validation: {error}"),
        };

        assert!(Validate::validate(&request).is_err());
    }
}
