use common::{error::AppResult, AppError, AppState};
use retrieval::{
    dto::{parse_memory_type, SearchFilters, SearchMode, SearchRequest},
    search::{hybrid, keyword, vector},
};
use serde::Deserialize;
use serde_json::json;
use uuid::Uuid;

use super::{MemoryToolResult, ToolDefinition};

const DEFAULT_LIMIT: u32 = 20;
const MAX_LIMIT: u32 = 100;

#[derive(Debug, Clone, Deserialize)]
pub struct SearchInput {
    pub query: String,
    #[serde(default)]
    pub search_type: SearchType,
    #[serde(default = "default_limit")]
    pub limit: u32,
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
        description: "Search MemoryOps memory units for the authenticated workspace.",
        input_schema: json!({
            "type": "object",
            "required": ["query"],
            "properties": {
                "query": { "type": "string" },
                "search_type": { "type": "string", "enum": ["hybrid", "keyword", "vector"], "default": "hybrid" },
                "limit": { "type": "integer", "minimum": 1, "maximum": MAX_LIMIT, "default": DEFAULT_LIMIT },
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
    let request = SearchRequest {
        query: input.query,
        workspace_id,
        mode,
        limit: Some(limit),
        offset: None,
        filters: search_filters(input.filters)?,
        scope: None,
        agent_id: None,
        user_id: None,
        repo: None,
        memory_types: None,
    };

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
