use std::collections::HashMap;

use axum::{
    extract::{Path, Query, State},
    Extension, Json,
};
use common::{auth::AuthContext, error::AppResult, AppError, AppState};
use serde::Deserialize;
use uuid::Uuid;

use crate::{
    dto::MemoryUnitDto,
    store::{self, FeedbackWrite},
};

use super::{resolve_workspace_id, workspace_id_param};

const DEFAULT_FEEDBACK_LIMIT: u32 = 20;
const MAX_FEEDBACK_LIMIT: u32 = 100;
const MAX_COMMENT_CHARS: usize = 500;

#[derive(Debug, Deserialize)]
pub struct SubmitFeedbackRequest {
    pub query_id: String,
    pub rating: i16,
    pub agent_id: Option<String>,
    pub user_id: Option<String>,
    pub comment: Option<String>,
}

#[axum::debug_handler]
pub async fn handle_submit_feedback(
    State(state): State<AppState>,
    auth: Option<Extension<AuthContext>>,
    Path(memory_id): Path<Uuid>,
    Query(params): Query<HashMap<String, String>>,
    Json(request): Json<SubmitFeedbackRequest>,
) -> AppResult<Json<MemoryUnitDto>> {
    validate_feedback_request(&request)?;
    let auth_context = auth.as_ref().map(|extension| &extension.0);
    let workspace_id = resolve_workspace_id(auth_context, workspace_id_param(&params)?)?;
    let feedback = FeedbackWrite {
        query_id: &request.query_id,
        agent_id: request.agent_id.as_deref(),
        user_id: request.user_id.as_deref(),
        rating: request.rating,
        comment: request.comment.as_deref(),
    };
    let memory =
        store::submit_retrieval_feedback(&state.db, workspace_id, memory_id, &feedback).await?;

    Ok(Json(MemoryUnitDto::from(memory)))
}

#[axum::debug_handler]
pub async fn handle_list_feedback(
    State(state): State<AppState>,
    auth: Option<Extension<AuthContext>>,
    Path(memory_id): Path<Uuid>,
    Query(params): Query<HashMap<String, String>>,
) -> AppResult<Json<common::models::FeedbackResponse>> {
    let auth_context = auth.as_ref().map(|extension| &extension.0);
    let workspace_id = resolve_workspace_id(auth_context, workspace_id_param(&params)?)?;
    let limit = u32_query_param(&params, "limit")?
        .unwrap_or(DEFAULT_FEEDBACK_LIMIT)
        .clamp(1, MAX_FEEDBACK_LIMIT);
    let offset = u32_query_param(&params, "offset")?.unwrap_or(0);
    let response =
        store::list_retrieval_feedback(&state.db, workspace_id, memory_id, limit, offset).await?;

    Ok(Json(response))
}

pub fn validate_feedback_request(request: &SubmitFeedbackRequest) -> AppResult<()> {
    if !(-1..=1).contains(&request.rating) {
        return Err(AppError::Unprocessable(
            "rating must be one of -1, 0, 1".to_owned(),
        ));
    }

    if request
        .comment
        .as_ref()
        .is_some_and(|comment| comment.chars().count() > MAX_COMMENT_CHARS)
    {
        return Err(AppError::Unprocessable(
            "comment must be 500 characters or fewer".to_owned(),
        ));
    }

    Ok(())
}

fn u32_query_param(params: &HashMap<String, String>, name: &'static str) -> AppResult<Option<u32>> {
    let Some(raw) = params.get(name) else {
        return Ok(None);
    };

    raw.parse::<u32>()
        .map(Some)
        .map_err(|_| AppError::Validation(format!("invalid {name} query parameter")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validation_rejects_invalid_rating() {
        let request = SubmitFeedbackRequest {
            query_id: "query".to_owned(),
            rating: 2,
            agent_id: None,
            user_id: None,
            comment: None,
        };

        let error = match validate_feedback_request(&request) {
            Ok(()) => panic!("invalid rating should be rejected"),
            Err(error) => error,
        };

        assert!(matches!(error, AppError::Unprocessable(_)));
    }

    #[test]
    fn validation_rejects_comment_over_500_chars() {
        let request = SubmitFeedbackRequest {
            query_id: "query".to_owned(),
            rating: 1,
            agent_id: None,
            user_id: None,
            comment: Some("x".repeat(501)),
        };

        let error = match validate_feedback_request(&request) {
            Ok(()) => panic!("long comment should be rejected"),
            Err(error) => error,
        };

        assert!(matches!(error, AppError::Unprocessable(_)));
    }
}
