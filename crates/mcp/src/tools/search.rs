use common::{error::AppResult, services::WorkspaceConfigService, AppError, AppState};
use retrieval::{
    dto::{parse_memory_type, SearchFilters, SearchMode, SearchRequest},
    search::{hybrid, keyword, vector},
};
use serde::Deserialize;
use serde_json::json;
use uuid::Uuid;

use super::{retrieve, MemoryToolResult, ToolDefinition};

const DEFAULT_LIMIT: u32 = 20;
const MAX_LIMIT: u32 = 100;

#[derive(Debug, Clone, Deserialize)]
pub struct SearchInput {
    pub query: String,
    #[serde(default)]
    pub search_type: SearchType,
    #[serde(default = "default_limit")]
    pub limit: u32,
    #[serde(default)]
    pub include_workspace_pool: bool,
    #[serde(default = "default_true")]
    pub include_master_memory: bool,
    pub agent_id: Option<String>,
    pub user_id: Option<String>,
    pub repo: Option<String>,
    pub filters: Option<SearchInputFilters>,
}

#[derive(Debug, Clone, Copy, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum SearchType {
    #[default]
    Hybrid,
    Keyword,
    Vector,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SearchInputFilters {
    pub tags: Option<Vec<String>>,
    pub memory_type: Option<String>,
}

pub fn definition() -> ToolDefinition {
    ToolDefinition {
        name: "memory_search",
        description: "Search MemoryOps memory units for the authenticated workspace, optionally scoped by user, agent, or repo.",
        input_schema: json!({
            "type": "object",
            "required": ["query"],
            "properties": {
                "query": { "type": "string" },
                "search_type": { "type": "string", "enum": ["hybrid", "keyword", "vector"], "default": "hybrid" },
                "limit": { "type": "integer", "minimum": 1, "maximum": MAX_LIMIT, "default": DEFAULT_LIMIT },
                "include_workspace_pool": { "type": "boolean", "default": false },
                "include_master_memory": { "type": "boolean", "default": true },
                "agent_id": { "type": "string", "description": "Optional agent scope." },
                "user_id": { "type": "string", "description": "Optional user scope." },
                "repo": { "type": "string", "description": "Optional repository/project scope such as owner/name." },
                "filters": {
                    "type": "object",
                    "properties": {
                        "tags": { "type": "array", "items": { "type": "string" } },
                        "memory_type": { "type": "string", "enum": ["episodic", "semantic"] }
                    }
                }
            }
        }),
    }
}

pub async fn run(
    state: &AppState,
    workspace_id: Uuid,
    input: SearchInput,
) -> AppResult<Vec<MemoryToolResult>> {
    if input.query.trim().is_empty() {
        return Err(AppError::Validation("query is required".to_owned()));
    }

    let limit = input.limit.clamp(1, MAX_LIMIT);
    let mode = SearchMode::from(input.search_type);
    let agent_id = retrieve::normalize_scope_value(input.agent_id);
    let user_id = retrieve::normalize_scope_value(input.user_id);
    let repo = retrieve::normalize_scope_value(input.repo);
    let mut request = SearchRequest {
        query: input.query,
        workspace_id,
        mode,
        limit: Some(limit),
        offset: None,
        filters: search_filters(input.filters)?,
        scope: retrieve::scope_filter(agent_id, user_id, repo),
        agent_id: None,
        user_id: None,
        repo: None,
        memory_types: None,
        as_of: None,
        include_workspace_pool: input.include_workspace_pool,
        include_master_memory: input.include_master_memory,
        inherited_workspace_pool_agent_ids: Vec::new(),
    };
    let workspace_config = WorkspaceConfigService::new(state.db.clone())
        .load(workspace_id)
        .await?;
    request.apply_workspace_config(&workspace_config);

    let results = match mode {
        SearchMode::Vector => vector::vector_search_results(state, &request, limit).await?,
        SearchMode::Keyword => keyword::keyword_search(state, &request, limit).await?,
        SearchMode::Hybrid => hybrid::hybrid_search(state, &request, limit).await?,
    };

    Ok(results
        .into_iter()
        .map(MemoryToolResult::from_memory_result)
        .collect())
}

fn search_filters(input: Option<SearchInputFilters>) -> AppResult<Option<SearchFilters>> {
    let Some(input) = input else {
        return Ok(None);
    };

    Ok(Some(SearchFilters {
        memory_type: match input.memory_type {
            Some(memory_type) => Some(parse_memory_type(&memory_type)?),
            None => None,
        },
        source: None,
        min_importance: None,
        pinned: None,
        tags: input.tags,
        agent_id: None,
        user_id: None,
        repo: None,
    }))
}

impl From<SearchType> for SearchMode {
    fn from(value: SearchType) -> Self {
        match value {
            SearchType::Hybrid => SearchMode::Hybrid,
            SearchType::Keyword => SearchMode::Keyword,
            SearchType::Vector => SearchMode::Vector,
        }
    }
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
        let properties = schema
            .get("properties")
            .and_then(serde_json::Value::as_object)
            .expect("properties should exist");

        assert!(properties.contains_key("agent_id"));
        assert!(properties.contains_key("user_id"));
        assert!(properties.contains_key("repo"));
        assert!(properties.contains_key("include_master_memory"));
    }
}
