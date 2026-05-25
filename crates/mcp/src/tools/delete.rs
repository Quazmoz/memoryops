use common::{error::AppResult, AppError, AppState};
use retrieval::{services::MemoryDeletionService, store};
use serde::{Deserialize, Serialize};
use serde_json::json;
use uuid::Uuid;

use super::ToolDefinition;

#[derive(Debug, Clone, Deserialize)]
pub struct DeleteInput {
    pub memory_id: Uuid,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct DeleteOutput {
    pub deleted: bool,
    pub memory_id: Uuid,
}

pub fn definition() -> ToolDefinition {
    ToolDefinition {
        name: "memory_delete",
        description: "Soft-delete a MemoryOps memory and remove its vector point.",
        input_schema: json!({
            "type": "object",
            "required": ["memory_id"],
            "properties": {
                "memory_id": { "type": "string", "format": "uuid", "description": "ID of the memory to soft-delete" }
            }
        }),
    }
}

pub async fn run(
    state: &AppState,
    workspace_id: Uuid,
    input: DeleteInput,
) -> AppResult<DeleteOutput> {
    let existing =
        store::get_memory_unit_by_id_including_deleted(&state.db, input.memory_id, workspace_id)
            .await?
            .ok_or_else(|| AppError::NotFound {
                resource: format!("memory:{}", input.memory_id),
            })?;

    if existing.deleted_at.is_some() {
        return Err(AppError::Conflict(format!(
            "memory:{} is already deleted",
            input.memory_id
        )));
    }

    MemoryDeletionService::new(
        state,
        processor::embedder::COLLECTION_NAME,
        "MCP memory_delete",
    )
    .soft_delete_required(input.memory_id, workspace_id)
    .await?;

    Ok(DeleteOutput {
        deleted: true,
        memory_id: input.memory_id,
    })
}
