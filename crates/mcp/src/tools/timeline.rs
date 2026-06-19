use chrono::{DateTime, Utc};
use common::{error::AppResult, services::WorkspaceConfigService, AppError, AppState};
use retrieval::{
    dto::{SearchMode, SearchRequest},
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
    #[serde(default)]
    pub include_workspace_pool: bool,
    #[serde(default = "default_true")]
    pub include_master_memory: bool,
    pub agent_id: Option<String>,
    pub user_id: Option<String>,
    pub repo: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct TimelineOutput {
    pub as_of: DateTime<Utc>,
    pub memories: Vec<MemoryToolResult>,
}

pub fn definition() -> ToolDefinition {
    ToolDefinition {
        name: "memory_timeline",
        description: "Retrieve memories as they existed at a past timestamp, optionally scoped by user, agent, or repo.",
        input_schema: json!({
            "type": "object",
            "required": ["query", "as_of"],
            "properties": {
                "query": { "type": "string", "description": "Natural language query" },
                "as_of": { "type": "string", "format": "date-time", "description": "Reconstruct memory state at this timestamp" },
                "limit": { "type": "integer", "default": DEFAULT_LIMIT, "maximum": MAX_LIMIT },
                "include_workspace_pool": { "type": "boolean", "default": false },
                "include_master_memory": { "type": "boolean", "default": true },
                "agent_id": { "type": "string", "description": "Optional agent scope" },
                "user_id": { "type": "string", "description": "Optional user scope" },
                "repo": { "type": "string", "description": "Optional repository/project scope such as owner/name" }
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
    let agent_id = retrieve::normalize_scope_value(input.agent_id);
    let user_id = retrieve::normalize_scope_value(input.user_id);
    let repo = retrieve::normalize_scope_value(input.repo);
    let mut request = SearchRequest {
        query: input.query,
        workspace_id,
        mode: SearchMode::Hybrid,
        limit: Some(limit),
        offset: None,
        filters: None,
        scope: retrieve::scope_filter(agent_id, user_id, repo),
        agent_id: None,
        user_id: None,
        repo: None,
        memory_types: None,
        as_of: Some(input.as_of),
        include_workspace_pool: input.include_workspace_pool,
        include_master_memory: input.include_master_memory,
        inherited_workspace_pool_agent_ids: Vec::new(),
    };
    let workspace_config = WorkspaceConfigService::new(state.db.clone())
        .load(workspace_id)
        .await?;
    request.apply_workspace_config(&workspace_config);
    let results =
        hybrid::hybrid_search_with_config(state, &request, limit, &workspace_config).await?;
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

fn default_true() -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn definition_exposes_scope_properties() {
        let schema = definition().input_schema;
        let Some(properties) = schema
            .get("properties")
            .and_then(serde_json::Value::as_object)
        else {
            panic!("properties should exist");
        };

        assert!(properties.contains_key("agent_id"));
        assert!(properties.contains_key("user_id"));
        assert!(properties.contains_key("repo"));
        assert!(properties.contains_key("include_master_memory"));
    }
}
