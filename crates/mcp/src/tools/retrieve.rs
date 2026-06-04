use common::{error::AppResult, services::WorkspaceConfigService, AppError, AppState};
use retrieval::{
    dto::{SearchMode, SearchRequest},
    search::hybrid,
    store::{self, FeedbackWrite},
};
use serde::Deserialize;
use serde_json::json;
use uuid::Uuid;

use super::{MemoryToolResult, ToolToolResult, ToolDefinition};

const DEFAULT_LIMIT: u32 = 10;
const MAX_LIMIT: u32 = 50;

#[derive(Debug, Clone, Deserialize)]
pub struct RetrieveInput {
    pub query: String,
    #[serde(default = "default_limit")]
    pub limit: u32,
    #[serde(default)]
    pub min_score: f32,
    #[serde(default)]
    pub include_workspace_pool: bool,
    pub feedback: Option<RetrieveFeedbackInput>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RetrieveFeedbackInput {
    pub query_id: String,
    pub ratings: Vec<RetrieveFeedbackRating>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RetrieveFeedbackRating {
    pub memory_id: Uuid,
    pub rating: i16,
}

#[derive(Debug, Clone, serde::Serialize, PartialEq)]
pub struct RetrieveOutput {
    pub memories: Vec<MemoryToolResult>,
    pub tools: Vec<ToolToolResult>,
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
                "min_score": { "type": "number", "minimum": 0.0, "default": 0.0 },
                "include_workspace_pool": { "type": "boolean", "default": false },
                "feedback": {
                    "type": "object",
                    "required": ["query_id", "ratings"],
                    "properties": {
                        "query_id": { "type": "string" },
                        "ratings": {
                            "type": "array",
                            "items": {
                                "type": "object",
                                "required": ["memory_id", "rating"],
                                "properties": {
                                    "memory_id": { "type": "string", "format": "uuid" },
                                    "rating": { "type": "integer", "enum": [-1, 0, 1] }
                                }
                            }
                        }
                    }
                }
            }
        }),
    }
}

pub async fn run(
    state: &AppState,
    workspace_id: Uuid,
    input: RetrieveInput,
) -> AppResult<RetrieveOutput> {
    if input.query.trim().is_empty() {
        return Err(AppError::Validation("query is required".to_owned()));
    }

    let feedback = input.feedback.clone();
    let limit = input.limit.clamp(1, MAX_LIMIT);
    let mut request = SearchRequest {
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
        as_of: None,
        include_workspace_pool: input.include_workspace_pool,
        inherited_workspace_pool_agent_ids: Vec::new(),
    };
    let workspace_config = WorkspaceConfigService::new(state.db.clone())
        .load(workspace_id)
        .await?;
    request.apply_workspace_config(&workspace_config);
    let results = hybrid::hybrid_search(state, &request, limit).await?;
    let min_score = input.min_score.max(0.0);
    let token_budget = state.config.retrieval.default_token_budget;

    let memories = pack_results(results, min_score, token_budget, limit as usize);
    let tools = load_enabled_tools(state, workspace_id).await?;
    let output = RetrieveOutput { memories, tools };

    if let Some(feedback) = feedback.as_ref() {
        submit_feedback_batch(state, workspace_id, feedback).await?;
    }

    Ok(output)
}

async fn submit_feedback_batch(
    state: &AppState,
    workspace_id: Uuid,
    feedback: &RetrieveFeedbackInput,
) -> AppResult<()> {
    for rating in &feedback.ratings {
        if !(-1..=1).contains(&rating.rating) {
            return Err(AppError::Validation(
                "feedback rating must be one of -1, 0, 1".to_owned(),
            ));
        }

        let write = FeedbackWrite {
            query_id: &feedback.query_id,
            agent_id: None,
            user_id: None,
            rating: rating.rating,
            comment: None,
        };
        store::submit_retrieval_feedback(&state.db, workspace_id, rating.memory_id, &write).await?;
    }

    Ok(())
}

async fn load_enabled_tools(
    state: &AppState,
    workspace_id: Uuid,
) -> AppResult<Vec<ToolToolResult>> {
    sqlx::query_as::<_, ToolRow>(
        r#"
        SELECT name, description, endpoint_url, http_method, input_schema, output_schema, version
        FROM workspace_tools
        WHERE workspace_id = $1
          AND enabled = TRUE
          AND scope_visibility IN ('workspace', 'published')
        ORDER BY name ASC
        "#,
    )
    .bind(workspace_id)
    .fetch_all(&state.db)
    .await
    .map(|rows| rows.into_iter().map(ToolToolResult::from).collect())
    .map_err(AppError::Database)
}

#[derive(Debug, sqlx::FromRow)]
struct ToolRow {
    name: String,
    description: String,
    endpoint_url: String,
    http_method: String,
    input_schema: serde_json::Value,
    output_schema: serde_json::Value,
    version: i32,
}

impl From<ToolRow> for ToolToolResult {
    fn from(row: ToolRow) -> Self {
        Self {
            name: row.name,
            description: row.description,
            endpoint_url: row.endpoint_url,
            http_method: row.http_method,
            input_schema: row.input_schema,
            output_schema: row.output_schema,
            version: row.version,
        }
    }
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
            .unwrap_or_else(|| estimate_tokens_lossy(&result.memory.content));
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

fn estimate_tokens_lossy(content: &str) -> usize {
    common::tokens::estimate_tokens(content).unwrap_or_else(|error| {
        tracing::warn!(error = ?error, "failed to estimate tokens with shared tokenizer; using byte heuristic");
        (content.len() / 4).max(1)
    })
}

fn default_limit() -> u32 {
    DEFAULT_LIMIT
}
