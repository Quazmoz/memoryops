use common::{error::AppResult, AppError, AppState};
use processor::worker::enqueue_slow_job;
use retrieval::store::{self, MemoryUnitPatch};
use serde::Deserialize;
use serde_json::json;
use uuid::Uuid;

use super::{MemoryToolResult, ToolDefinition};

#[derive(Debug, Clone, Deserialize)]
pub struct UpdateInput {
    pub memory_id: Uuid,
    pub content: Option<String>,
    pub tags: Option<Vec<String>>,
    pub importance_score: Option<f32>,
}

pub fn definition() -> ToolDefinition {
    ToolDefinition {
        name: "memory_update",
        description: "Update memory content, tags, or importance for the authenticated workspace.",
        input_schema: json!({
            "type": "object",
            "required": ["memory_id"],
            "properties": {
                "memory_id": { "type": "string", "format": "uuid" },
                "content": { "type": "string", "description": "New content to replace existing content" },
                "tags": { "type": "array", "items": { "type": "string" }, "description": "Optional replacement tag list" },
                "importance_score": { "type": "number", "minimum": 0.0, "maximum": 1.0, "description": "Optional importance override" }
            }
        }),
    }
}

pub async fn run(
    state: &AppState,
    workspace_id: Uuid,
    input: UpdateInput,
) -> AppResult<MemoryToolResult> {
    if input.content.is_none() && input.tags.is_none() && input.importance_score.is_none() {
        return Err(AppError::Validation(
            "at least one of content, tags, or importance_score is required".to_owned(),
        ));
    }
    if input
        .content
        .as_ref()
        .is_some_and(|content| content.trim().is_empty())
    {
        return Err(AppError::Validation("content is required".to_owned()));
    }

    let content_changed = input.content.is_some();
    let patch = MemoryUnitPatch {
        content: input.content.as_deref(),
        tags: input.tags.as_deref(),
        importance_score: input.importance_score,
        edited_by: "mcp",
    };
    let updated =
        store::update_memory_unit_patch(&state.db, input.memory_id, workspace_id, &patch).await?;

    if content_changed {
        match state.redis.get().await {
            Ok(mut conn) => {
                if let Err(error) = enqueue_slow_job(&mut *conn, updated.id, updated.workspace_id, 0).await {
                    tracing::warn!(error = ?error, memory_id = %updated.id, "failed to enqueue MCP-updated memory for re-embedding");
                }
            }
            Err(error) => tracing::warn!(error = ?error, memory_id = %updated.id, "failed to get Redis connection for MCP update enqueue"),
        }
    }

    Ok(MemoryToolResult::from_memory_unit(updated, 1.0))
}
