use common::{error::AppResult, services::WorkspaceConfigService, AppError, AppState};
use retrieval::{
    dto::{ScopeFilter, SearchMode, SearchRequest},
    search::hybrid,
    store::{self, FeedbackWrite},
};
use serde::Deserialize;
use serde_json::json;
use uuid::Uuid;

use super::{MemoryToolResult, ToolDefinition, ToolToolResult};

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
    #[serde(default = "default_true")]
    pub include_master_memory: bool,
    pub agent_id: Option<String>,
    pub user_id: Option<String>,
    pub repo: Option<String>,
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
        description: "Retrieve token-packed MemoryOps context for the authenticated workspace, including scoped user/agent/repo memory plus master workspace memory by default.",
        input_schema: json!({
            "type": "object",
            "required": ["query"],
            "description": "workspace_id is injected from the authenticated MCP session, not accepted in tool input.",
            "properties": {
                "query": { "type": "string" },
                "limit": { "type": "integer", "minimum": 1, "maximum": MAX_LIMIT, "default": DEFAULT_LIMIT },
                "min_score": { "type": "number", "minimum": 0.0, "default": 0.0 },
                "include_workspace_pool": { "type": "boolean", "default": false },
                "include_master_memory": { "type": "boolean", "default": true },
                "agent_id": { "type": "string", "description": "Optional agent scope. Matching agent memory is retrieved with master workspace memory." },
                "user_id": { "type": "string", "description": "Optional user scope. Matching user memory is retrieved with master workspace memory." },
                "repo": { "type": "string", "description": "Optional repository/project scope such as owner/name." },
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
    let agent_id = normalize_scope_value(input.agent_id);
    let user_id = normalize_scope_value(input.user_id);
    let repo = normalize_scope_value(input.repo);
    let mut request = SearchRequest {
        query: input.query,
        workspace_id,
        mode: SearchMode::Hybrid,
        limit: Some(limit),
        offset: None,
        filters: None,
        scope: scope_filter(agent_id.clone(), user_id.clone(), repo.clone()),
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
    let results = hybrid::hybrid_search_with_config(state, &request, limit, &workspace_config).await?;
    let min_score = input.min_score.max(0.0);
    let token_budget = state.config.retrieval.default_token_budget;

    let memories = pack_results(results, min_score, token_budget, limit as usize);
    let tools = load_enabled_tools(state, workspace_id).await?;
    let output = RetrieveOutput { memories, tools };

    if let Some(feedback) = feedback.as_ref() {
        submit_feedback_batch(
            state,
            workspace_id,
            feedback,
            agent_id.as_deref(),
            user_id.as_deref(),
        )
        .await?;
    }

    Ok(output)
}

async fn submit_feedback_batch(
    state: &AppState,
    workspace_id: Uuid,
    feedback: &RetrieveFeedbackInput,
    agent_id: Option<&str>,
    user_id: Option<&str>,
) -> AppResult<()> {
    for rating in &feedback.ratings {
        if !(-1..=1).contains(&rating.rating) {
            return Err(AppError::Validation(
                "feedback rating must be one of -1, 0, 1".to_owned(),
            ));
        }

        let write = FeedbackWrite {
            query_id: &feedback.query_id,
            agent_id,
            user_id,
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
    output_schema: Option<serde_json::Value>,
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
            output_schema: row.output_schema.unwrap_or_else(|| json!({})),
            version: row.version,
        }
    }
}

pub(crate) fn pack_results(
    results: Vec<retrieval::dto::MemoryResult>,
    min_score: f32,
    token_budget: usize,
    max_items: usize,
) -> Vec<MemoryToolResult> {
    let mut memories = Vec::new();
    let mut total_tokens = 0_usize;
    for result in results {
        if result.score < min_score {
            continue;
        }
        let tokens = result.memory.token_count.unwrap_or(0).max(0) as usize;
        if token_budget > 0 && total_tokens.saturating_add(tokens) > token_budget {
            continue;
        }
        total_tokens = total_tokens.saturating_add(tokens);
        memories.push(MemoryToolResult::from_memory_result(result));
        if memories.len() >= max_items {
            break;
        }
    }
    memories
}

pub(crate) fn scope_filter(
    agent_id: Option<String>,
    user_id: Option<String>,
    repo: Option<String>,
) -> Option<ScopeFilter> {
    let scope = ScopeFilter {
        agent_id,
        user_id,
        repo,
    };
    if scope.is_empty() {
        None
    } else {
        Some(scope)
    }
}

pub(crate) fn normalize_scope_value(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
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
    fn scope_filter_omits_empty_scope() {
        assert!(scope_filter(None, None, None).is_none());
    }

    #[test]
    fn scope_filter_keeps_user_agent_and_repo() {
        let scope = scope_filter(
            Some("agent-1".to_owned()),
            Some("user-1".to_owned()),
            Some("Quazmoz/memoryops".to_owned()),
        )
        .expect("scope should exist");

        assert_eq!(scope.agent_id.as_deref(), Some("agent-1"));
        assert_eq!(scope.user_id.as_deref(), Some("user-1"));
        assert_eq!(scope.repo.as_deref(), Some("Quazmoz/memoryops"));
    }

    #[test]
    fn normalize_scope_value_trims_and_drops_empty_values() {
        assert_eq!(normalize_scope_value(Some(" user-1 ".to_owned())).as_deref(), Some("user-1"));
        assert!(normalize_scope_value(Some("   ".to_owned())).is_none());
        assert!(normalize_scope_value(None).is_none());
    }
}
