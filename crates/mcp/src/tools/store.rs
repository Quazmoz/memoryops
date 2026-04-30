use chrono::{DateTime, Utc};
use common::{error::AppResult, AppError, AppState};
use ingestion::{ingest_observation, ObservationInput};
use serde::{Deserialize, Serialize};
use serde_json::json;
use uuid::Uuid;

use super::ToolDefinition;

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
        description: "Store an episodic MemoryOps memory for the authenticated workspace.",
        input_schema: json!({
            "type": "object",
            "required": ["content"],
            "properties": {
                "content": { "type": "string", "minLength": 1, "maxLength": 8000 },
                "agent_id": { "type": "string", "default": DEFAULT_AGENT_ID },
                "tags": { "type": "array", "items": { "type": "string" }, "maxItems": 20 },
                "importance": { "type": "number", "minimum": 0.0, "maximum": 1.0, "default": DEFAULT_IMPORTANCE },
                "user_id": { "type": "string" },
                "repo": { "type": "string" },
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
        user_id: input.user_id,
        repo: input.repo,
        tags: Some(input.tags),
        importance: Some(input.importance),
        source_ref: input.source_ref,
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
