use common::{error::AppResult, AppError, AppState};
use retrieval::store::{self, FeedbackWrite};
use serde::{Deserialize, Serialize};
use serde_json::json;
use uuid::Uuid;

use super::ToolDefinition;

const MAX_COMMENT_CHARS: usize = 500;

#[derive(Debug, Clone, Deserialize)]
pub struct FeedbackInput {
    pub memory_id: Uuid,
    pub query_id: Uuid,
    pub rating: i16,
    pub agent_id: String,
    pub comment: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct FeedbackOutput {
    pub memory_id: Uuid,
    pub new_relevance_score: f32,
}

pub fn definition() -> ToolDefinition {
    ToolDefinition {
        name: "memory_feedback",
        description: "Rate a retrieved memory so future retrieval can learn from agent feedback.",
        input_schema: json!({
            "type": "object",
            "required": ["memory_id", "query_id", "rating", "agent_id"],
            "properties": {
                "memory_id": { "type": "string", "format": "uuid" },
                "query_id": { "type": "string", "format": "uuid", "description": "Retrieval trace query_id that surfaced this memory" },
                "rating": { "type": "integer", "enum": [-1, 0, 1], "description": "-1 = not helpful, 0 = neutral, 1 = helpful" },
                "agent_id": { "type": "string", "description": "Agent submitting feedback" },
                "comment": { "type": "string", "description": "Optional free-text note" }
            }
        }),
    }
}

pub async fn run(
    state: &AppState,
    workspace_id: Uuid,
    input: FeedbackInput,
) -> AppResult<FeedbackOutput> {
    if !(-1..=1).contains(&input.rating) {
        return Err(AppError::Unprocessable(
            "rating must be one of -1, 0, 1".to_owned(),
        ));
    }
    if input
        .comment
        .as_ref()
        .is_some_and(|comment| comment.chars().count() > MAX_COMMENT_CHARS)
    {
        return Err(AppError::Unprocessable(
            "comment must be 500 characters or fewer".to_owned(),
        ));
    }

    let query_id = input.query_id.to_string();
    let feedback = FeedbackWrite {
        query_id: &query_id,
        agent_id: Some(input.agent_id.as_str()),
        user_id: None,
        rating: input.rating,
        comment: input.comment.as_deref(),
    };
    let updated =
        store::submit_retrieval_feedback(&state.db, workspace_id, input.memory_id, &feedback)
            .await?;

    Ok(FeedbackOutput {
        memory_id: input.memory_id,
        new_relevance_score: updated.relevance_score as f32,
    })
}
