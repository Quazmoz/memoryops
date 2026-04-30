use chrono::{DateTime, Utc};
use common::{error::AppResult, AppError, AppState};
use retrieval::{
    dto::{ScopeFilter, SearchMode, SearchRequest},
    search::hybrid,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use uuid::Uuid;

use super::{retrieve, MemoryToolResult, ToolDefinition};

const DEFAULT_LIMIT: u32 = 10;
const MAX_LIMIT: u32 = 50;

#[derive(Debug, Clone, Deserialize)]
pub struct TimelineInput {
    pub query: String,
    pub as_of: DateTime<Utc>,
    #[serde(default = "default_limit")]
    pub limit: u32,
    pub agent_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct TimelineOutput {
    pub as_of: DateTime<Utc>,
    pub memories: Vec<MemoryToolResult>,
}

pub fn definition() -> ToolDefinition {
    ToolDefinition {
        name: "memory_timeline",
        description: "Retrieve memories as they existed at a past timestamp. Useful for incident post-mortems or understanding what the agent knew at a specific moment.",
        input_schema: json!({
            "type": "object",
            "required": ["query", "as_of"],
            "properties": {
                "query": { "type": "string", "description": "Natural language query" },
                "as_of": { "type": "string", "format": "date-time", "description": "Reconstruct memory state at this timestamp" },
                "limit": { "type": "integer", "default": DEFAULT_LIMIT, "maximum": MAX_LIMIT },
                "agent_id": { "type": "string", "description": "Optional scope filter" }
            }
        }),
    }
}

pub async fn run(
    state: &AppState,
    workspace_id: Uuid,
    input: TimelineInput,
) -> AppResult<TimelineOutput> {
    if input.query.trim().is_empty() {
        return Err(AppError::Validation("query is required".to_owned()));
    }

    let limit = input.limit.clamp(1, MAX_LIMIT);
    let mut request = SearchRequest {
        query: input.query,
        workspace_id,
        mode: SearchMode::Hybrid,
        limit: Some(limit),
        offset: None,
        filters: None,
        scope: input.agent_id.map(|agent_id| ScopeFilter {
            agent_id: Some(agent_id),
            user_id: None,
            repo: None,
        }),
        agent_id: None,
        user_id: None,
        repo: None,
        memory_types: None,
        as_of: Some(input.as_of),
        include_workspace_pool: false,
        inherited_workspace_pool_agent_ids: Vec::new(),
    };
    request.apply_workspace_config(&retrieve::load_workspace_config(state, workspace_id).await?);
    let results = hybrid::hybrid_search(state, &request, limit).await?;
    let memories = retrieve::pack_results(
        results,
        0.0,
        state.config.retrieval.default_token_budget,
        limit as usize,
    );

    Ok(TimelineOutput {
        as_of: input.as_of,
        memories,
    })
}

fn default_limit() -> u32 {
    DEFAULT_LIMIT
}
