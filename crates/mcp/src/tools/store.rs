use chrono::{DateTime, Utc};
use common::{error::AppResult, AppError, AppState};
use ingestion::{ingest_observation, ObservationInput};
use serde::{Deserialize, Serialize};
use serde_json::json;
use uuid::Uuid;

use super::{retrieve, ToolDefinition};

const DEFAULT_AGENT_ID: &str = "mcp-agent";
const DEFAULT_IMPORTANCE: f32 = 0.5;

#[derive(Debug, Clone, Deserialize)]
pub struct StoreInput {
    pub content: String,
    #[serde(default = "default_agent_id")]
    pub agent_id: String,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default = "default_importance")]
    pub importance: f32,
    pub user_id: Option<String>,
    pub repo: Option<String>,
    pub source_ref: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct StoreOutput {
    pub id: Uuid,
    pub created_at: DateTime<Utc>,
}

pub fn definition() -> ToolDefinition {
    ToolDefinition {
        name: "memory_store",
        description: "Store an episodic MemoryOps memory for the authenticated workspace. Supply user_id, agent_id, and/or repo to make the memory retrievable only for that scope plus master workspace context.",
        input_schema: json!({
            "type": "object",
            "required": ["content"],
            "properties": {
                "content": { "type": "string", "minLength": 1, "maxLength": 8000 },
                "agent_id": { "type": "string", "default": DEFAULT_AGENT_ID, "description": "Agent scope for this memory. Defaults to mcp-agent." },
                "tags": { "type": "array", "items": { "type": "string" }, "maxItems": 20 },
                "importance": { "type": "number", "minimum": 0.0, "maximum": 1.0, "default": DEFAULT_IMPORTANCE },
                "user_id": { "type": "string", "description": "Optional user scope for this memory." },
                "repo": { "type": "string", "description": "Optional repository/project scope such as owner/name." },
                "source_ref": { "type": "string" }
            }
        }),
    }
}

pub async fn run(
    state: &AppState,
    workspace_id: Uuid,
    input: StoreInput,
) -> AppResult<StoreOutput> {
    if input.content.trim().is_empty() {
        return Err(AppError::Validation("content is required".to_owned()));
    }

    let agent_id = normalize_agent_id(&input.agent_id);
    let observation_input = ObservationInput {
        content: input.content,
        agent_id,
        user_id: retrieve::normalize_scope_value(input.user_id),
        repo: retrieve::normalize_scope_value(input.repo),
        tags: Some(input.tags),
        importance: Some(input.importance),
        source_ref: retrieve::normalize_scope_value(input.source_ref),
        scope_id: None,
    };

    let output = ingest_observation(state, workspace_id, observation_input).await?;
    Ok(StoreOutput {
        id: output.id,
        created_at: Utc::now(),
    })
}

fn normalize_agent_id(agent_id: &str) -> String {
    let trimmed = agent_id.trim();
    if trimmed.is_empty() {
        DEFAULT_AGENT_ID.to_owned()
    } else {
        trimmed.to_owned()
    }
}

fn default_agent_id() -> String {
    DEFAULT_AGENT_ID.to_owned()
}

fn default_importance() -> f32 {
    DEFAULT_IMPORTANCE
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_agent_id_uses_default_for_blank_values() {
        assert_eq!(normalize_agent_id("   "), DEFAULT_AGENT_ID);
        assert_eq!(normalize_agent_id(" agent-1 "), "agent-1");
    }
}
