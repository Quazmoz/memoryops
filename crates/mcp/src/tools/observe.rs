use chrono::{DateTime, Utc};
use common::{error::AppResult, AppError, AppState};
use ingestion::{ingest_observation, ObservationInput};
use retrieval::store::{self, ObservationListQuery};
use serde::{Deserialize, Serialize};
use serde_json::json;
use uuid::Uuid;

use super::ToolDefinition;

const DEFAULT_LIMIT: u32 = 20;
const MAX_LIMIT: u32 = 100;
const DEFAULT_AGENT_ID: &str = "mcp-agent";

#[derive(Debug, Clone, Deserialize)]
pub struct ObserveInput {
    pub content: String,
    pub source: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    pub scope_id: Option<Uuid>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ObserveOutput {
    pub id: Uuid,
    pub status: &'static str,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ListObservationsInput {
    #[serde(default = "default_limit")]
    pub limit: u32,
    pub scope_id: Option<Uuid>,
    pub since: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ObservationItem {
    pub id: Uuid,
    pub content: String,
    pub source: Option<String>,
    pub tags: Vec<String>,
    pub created_at: DateTime<Utc>,
    pub processed_at: Option<DateTime<Utc>>,
}

pub fn observe_definition() -> ToolDefinition {
    ToolDefinition {
        name: "memory_observe",
        description: "Ingest a raw observation into the current workspace. The processor will asynchronously consolidate it into memory units.",
        input_schema: json!({
            "type": "object",
            "required": ["content"],
            "properties": {
                "content": { "type": "string", "minLength": 1, "maxLength": 8000 },
                "source": { "type": "string" },
                "tags": { "type": "array", "items": { "type": "string" }, "maxItems": 20 },
                "scope_id": { "type": "string", "format": "uuid" }
            }
        }),
    }
}

pub fn list_observations_definition() -> ToolDefinition {
    ToolDefinition {
        name: "memory_list_observations",
        description: "List recent raw observations for the workspace, before they are processed into memory units.",
        input_schema: json!({
            "type": "object",
            "properties": {
                "limit": { "type": "integer", "minimum": 1, "maximum": MAX_LIMIT, "default": DEFAULT_LIMIT },
                "scope_id": { "type": "string", "format": "uuid" },
                "since": { "type": "string", "format": "date-time" }
            }
        }),
    }
}

pub async fn run_observe(
    state: &AppState,
    workspace_id: Uuid,
    input: ObserveInput,
) -> AppResult<ObserveOutput> {
    if input.content.trim().is_empty() {
        return Err(AppError::Validation("content is required".to_owned()));
    }

    let observation_input = ObservationInput {
        content: input.content,
        agent_id: DEFAULT_AGENT_ID.to_owned(),
        user_id: None,
        repo: None,
        tags: Some(input.tags),
        importance: None,
        source_ref: input.source,
        scope_id: input.scope_id,
    };

    let output = ingest_observation(state, workspace_id, observation_input).await?;
    Ok(ObserveOutput {
        id: output.id,
        status: output.status,
    })
}

pub async fn run_list_observations(
    state: &AppState,
    workspace_id: Uuid,
    input: ListObservationsInput,
) -> AppResult<Vec<ObservationItem>> {
    let query = ObservationListQuery {
        limit: input.limit.clamp(1, MAX_LIMIT),
        scope_id: input.scope_id,
        since: input.since,
    };
    let rows = store::list_observations(&state.db, workspace_id, &query).await?;

    Ok(rows
        .into_iter()
        .map(|row| ObservationItem {
            id: row.id,
            content: row.content,
            source: row.source,
            tags: row.tags,
            created_at: row.created_at,
            processed_at: row.processed_at,
        })
        .collect())
}

fn default_limit() -> u32 {
    DEFAULT_LIMIT
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_limit_is_20() {
        assert_eq!(default_limit(), 20);
    }

    #[test]
    fn observe_definition_name_matches() {
        assert_eq!(observe_definition().name, "memory_observe");
    }

    #[test]
    fn list_observations_definition_name_matches() {
        assert_eq!(list_observations_definition().name, "memory_list_observations");
    }
}
