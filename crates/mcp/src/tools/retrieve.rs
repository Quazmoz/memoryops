use common::{error::AppResult, AppError, AppState};
use retrieval::{
    dto::{SearchMode, SearchRequest},
    search::hybrid,
};
use serde::Deserialize;
use serde_json::json;
use uuid::Uuid;

use super::{MemoryToolResult, ToolDefinition};

const DEFAULT_LIMIT: u32 = 10;
const MAX_LIMIT: u32 = 50;

#[derive(Debug, Clone, Deserialize)]
pub struct RetrieveInput {
    pub query: String,
    #[serde(default = "default_limit")]
    pub limit: u32,
    #[serde(default)]
    pub min_score: f32,
}

pub fn definition() -> ToolDefinition {
    ToolDefinition {
        name: "memory_retrieve",
        description: "Retrieve token-packed MemoryOps context for the authenticated workspace.",
        input_schema: json!({
            "type": "object",
            "required": ["query"],
            "description": "workspace_id is injected from the authenticated MCP session, not accepted in tool input.",
            "properties": {
                "query": { "type": "string" },
                "limit": { "type": "integer", "minimum": 1, "maximum": MAX_LIMIT, "default": DEFAULT_LIMIT },
                "min_score": { "type": "number", "minimum": 0.0, "default": 0.0 }
            }
        }),
    }
}

pub async fn run(
    state: &AppState,
    workspace_id: Uuid,
    input: RetrieveInput,
) -> AppResult<Vec<MemoryToolResult>> {
    if input.query.trim().is_empty() {
        return Err(AppError::Validation("query is required".to_owned()));
    }

    let limit = input.limit.clamp(1, MAX_LIMIT);
    let request = SearchRequest {
        query: input.query,
        workspace_id,
        mode: SearchMode::Hybrid,
        limit: Some(limit),
        offset: None,
        filters: None,
        scope: None,
        agent_id: None,
        user_id: None,
        repo: None,
        memory_types: None,
    };
    let results = hybrid::hybrid_search(state, &request, limit).await?;
    let min_score = input.min_score.max(0.0);
    let token_budget = state.config.retrieval.default_token_budget;

    Ok(pack_results(
        results,
        min_score,
        token_budget,
        limit as usize,
    ))
}

pub fn pack_results(
    results: Vec<retrieval::MemoryResult>,
    min_score: f32,
    token_budget: usize,
    limit: usize,
) -> Vec<MemoryToolResult> {
    let mut total_tokens = 0_usize;
    let mut packed = Vec::new();

    for result in results {
        if result.score < min_score {
            continue;
        }

        let estimated_tokens = result
            .memory
            .token_count
            .and_then(|tokens| usize::try_from(tokens).ok())
            .unwrap_or_else(|| estimate_tokens(&result.memory.content));
        if total_tokens.saturating_add(estimated_tokens) > token_budget {
            continue;
        }

        total_tokens += estimated_tokens;
        packed.push(MemoryToolResult::from_memory_result(result));
        if packed.len() >= limit {
            break;
        }
    }

    packed
}

fn estimate_tokens(content: &str) -> usize {
    (content.len() / 4).max(1)
}

fn default_limit() -> u32 {
    DEFAULT_LIMIT
}
