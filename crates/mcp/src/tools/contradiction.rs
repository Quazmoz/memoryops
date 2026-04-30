use common::{error::AppResult, AppError, AppState};
use qdrant_client::qdrant::DeletePointsBuilder;
use retrieval::store;
use serde::{Deserialize, Serialize};
use serde_json::json;
use uuid::Uuid;

use super::ToolDefinition;

const DEFAULT_LIMIT: u32 = 10;
const MAX_LIMIT: u32 = 100;

#[derive(Debug, Clone, Deserialize)]
pub struct ListContradictionsInput {
    #[serde(default = "default_limit")]
    pub limit: u32,
    pub scope_id: Option<Uuid>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ContradictionItem {
    pub id: Uuid,
    pub memory_unit_a_id: Uuid,
    pub memory_unit_b_id: Uuid,
    pub description: String,
    pub detected_at: chrono::DateTime<chrono::Utc>,
    pub resolution_status: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ResolveContradictionInput {
    pub contradiction_id: Uuid,
    pub action: ResolveAction,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResolveAction {
    KeepA,
    KeepB,
    KeepBoth,
    DiscardBoth,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ResolveContradictionOutput {
    pub id: Uuid,
    pub memory_unit_a_id: Uuid,
    pub memory_unit_b_id: Uuid,
    pub description: String,
    pub detected_at: chrono::DateTime<chrono::Utc>,
    pub resolution_status: String,
}

pub fn list_definition() -> ToolDefinition {
    ToolDefinition {
        name: "memory_list_contradictions",
        description: "List unresolved contradictions detected between memory units.",
        input_schema: json!({
            "type": "object",
            "properties": {
                "limit": { "type": "integer", "minimum": 1, "maximum": MAX_LIMIT, "default": DEFAULT_LIMIT },
                "scope_id": { "type": "string", "format": "uuid" }
            }
        }),
    }
}

pub fn resolve_definition() -> ToolDefinition {
    ToolDefinition {
        name: "memory_resolve_contradiction",
        description: "Resolve a contradiction by specifying which memory unit to keep, or marking both as superseded.",
        input_schema: json!({
            "type": "object",
            "required": ["contradiction_id", "action"],
            "properties": {
                "contradiction_id": { "type": "string", "format": "uuid" },
                "action": { "type": "string", "enum": ["keep_a", "keep_b", "keep_both", "discard_both"] },
                "reason": { "type": "string" }
            }
        }),
    }
}

pub async fn run_list(
    state: &AppState,
    workspace_id: Uuid,
    input: ListContradictionsInput,
) -> AppResult<Vec<ContradictionItem>> {
    let rows = store::list_open_contradictions(
        &state.db,
        workspace_id,
        input.scope_id,
        input.limit.clamp(1, MAX_LIMIT),
    )
    .await?;

    Ok(rows
        .into_iter()
        .map(|row| ContradictionItem {
            id: row.id,
            memory_unit_a_id: row.memory_unit_a_id,
            memory_unit_b_id: row.memory_unit_b_id,
            description: row.description,
            detected_at: row.detected_at,
            resolution_status: row.resolution_status,
        })
        .collect())
}

pub async fn run_resolve(
    state: &AppState,
    workspace_id: Uuid,
    input: ResolveContradictionInput,
) -> AppResult<ResolveContradictionOutput> {
    let flag = store::get_contradiction_flag(&state.db, workspace_id, input.contradiction_id)
        .await?
        .ok_or_else(|| AppError::NotFound {
            resource: format!("contradiction:{}", input.contradiction_id),
        })?;

    if flag.resolution != "open" {
        return Err(AppError::Conflict(format!(
            "contradiction:{} is already resolved",
            input.contradiction_id
        )));
    }

    let reason = input
        .reason
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());

    let actor = "mcp";
    let (resolution, kept_id, discarded_id) = match input.action {
        ResolveAction::KeepA => {
            soft_delete_if_active(state, workspace_id, flag.memory_id_b).await?;
            ("keep_a", Some(flag.memory_id_a), Some(flag.memory_id_b))
        }
        ResolveAction::KeepB => {
            soft_delete_if_active(state, workspace_id, flag.memory_id_a).await?;
            ("keep_b", Some(flag.memory_id_b), Some(flag.memory_id_a))
        }
        ResolveAction::KeepBoth => ("accepted", None, None),
        ResolveAction::DiscardBoth => {
            soft_delete_if_active(state, workspace_id, flag.memory_id_a).await?;
            soft_delete_if_active(state, workspace_id, flag.memory_id_b).await?;
            ("dismissed", None, None)
        }
    };

    let resolved = store::resolve_contradiction_flag(
        &state.db,
        workspace_id,
        input.contradiction_id,
        &store::ContradictionResolutionUpdate {
            resolution,
            reason,
            resolved_by: actor,
            kept_memory_id: kept_id,
            discarded_memory_id: discarded_id,
        },
    )
    .await?
    .ok_or_else(|| AppError::NotFound {
        resource: format!("contradiction:{}", input.contradiction_id),
    })?;

    Ok(ResolveContradictionOutput {
        id: resolved.id,
        memory_unit_a_id: resolved.memory_unit_a_id,
        memory_unit_b_id: resolved.memory_unit_b_id,
        description: resolved.description,
        detected_at: resolved.detected_at,
        resolution_status: resolved.resolution_status,
    })
}

async fn soft_delete_if_active(state: &AppState, workspace_id: Uuid, memory_id: Uuid) -> AppResult<()> {
    let deleted = store::soft_delete_memory_unit(&state.db, memory_id, workspace_id).await?;
    if deleted.is_some() {
        let request = DeletePointsBuilder::new(processor::embedder::COLLECTION_NAME)
            .points([memory_id.to_string()])
            .wait(true);
        if let Err(error) = state.qdrant.delete_points(request).await {
            tracing::warn!(error = ?error, memory_id = %memory_id, "failed to delete Qdrant point for contradiction resolution");
        }
    }
    Ok(())
}

fn default_limit() -> u32 {
    DEFAULT_LIMIT
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn list_definition_name_matches() {
        assert_eq!(list_definition().name, "memory_list_contradictions");
    }

    #[test]
    fn resolve_definition_name_matches() {
        assert_eq!(resolve_definition().name, "memory_resolve_contradiction");
    }
}
