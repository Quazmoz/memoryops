use chrono::{DateTime, Utc};
use common::{
    error::AppResult,
    models::{MemoryType, MemoryUnit, ScopeVisibility, WorkspaceConfig},
    AppError,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use validator::Validate;

pub const DEFAULT_LIMIT: u32 = 20;
pub const MAX_LIMIT: u32 = 100;
pub const DEFAULT_OFFSET: u32 = 0;
pub const MIN_SCORE_THRESHOLD: f32 = 0.70;

#[derive(Debug, Clone, Default, Deserialize)]
pub struct ScopeFilter {
    pub agent_id: Option<String>,
    pub user_id: Option<String>,
    pub repo: Option<String>,
}

impl ScopeFilter {
    pub fn is_empty(&self) -> bool {
        self.agent_id.is_none() && self.user_id.is_none() && self.repo.is_none()
    }
}

#[derive(Debug, Clone, Deserialize, Validate)]
pub struct SearchRequest {
    #[validate(length(min = 1, max = 2000))]
    pub query: String,
    pub workspace_id: Uuid,
    #[serde(default)]
    pub mode: SearchMode,
    #[validate(range(min = 1, max = 100))]
    pub limit: Option<u32>,
    pub offset: Option<u32>,
    pub filters: Option<SearchFilters>,
    pub scope: Option<ScopeFilter>,
    pub agent_id: Option<String>,
    pub user_id: Option<String>,
    pub repo: Option<String>,
    pub memory_types: Option<Vec<String>>,
    pub as_of: Option<DateTime<Utc>>,
    #[serde(default)]
    pub include_workspace_pool: bool,
    #[serde(default = "default_true")]
    pub include_master_memory: bool,
    #[serde(default, skip_deserializing)]
    pub inherited_workspace_pool_agent_ids: Vec<String>,
}

impl SearchRequest {
    pub fn apply_workspace_config(&mut self, config: &WorkspaceConfig) {
        self.inherited_workspace_pool_agent_ids = normalized_agent_ids(&config.sub_agent_pools);
    }

    pub fn resolved_scope_filter(&self) -> Option<ScopeFilter> {
        let scope = ScopeFilter {
            agent_id: first_scope_value([
                self.agent_id.as_ref(),
                self.scope
                    .as_ref()
                    .and_then(|scope| scope.agent_id.as_ref()),
                self.filters
                    .as_ref()
                    .and_then(|filters| filters.agent_id.as_ref()),
            ]),
            user_id: first_scope_value([
                self.user_id.as_ref(),
                self.scope.as_ref().and_then(|scope| scope.user_id.as_ref()),
                self.filters
                    .as_ref()
                    .and_then(|filters| filters.user_id.as_ref()),
            ]),
            repo: first_scope_value([
                self.repo.as_ref(),
                self.scope.as_ref().and_then(|scope| scope.repo.as_ref()),
                self.filters
                    .as_ref()
                    .and_then(|filters| filters.repo.as_ref()),
            ]),
        };

        if scope.is_empty() {
            None
        } else {
            Some(scope)
        }
    }

    pub fn workspace_pool_access(&self) -> WorkspacePoolAccess {
        WorkspacePoolAccess {
            include_all_workspace: self.include_workspace_pool,
            include_master_memory: self.include_master_memory,
            inherited_agent_ids: normalized_agent_ids(&self.inherited_workspace_pool_agent_ids),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WorkspacePoolAccess {
    pub include_all_workspace: bool,
    pub include_master_memory: bool,
    pub inherited_agent_ids: Vec<String>,
}

impl WorkspacePoolAccess {
    pub fn includes_any_workspace_pool(&self) -> bool {
        self.include_all_workspace
            || self.include_master_memory
            || !self.inherited_agent_ids.is_empty()
    }
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, Default, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum SearchMode {
    Vector,
    Keyword,
    #[default]
    Hybrid,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SearchFilters {
    pub memory_type: Option<MemoryType>,
    pub source: Option<String>,
    pub min_importance: Option<f32>,
    pub pinned: Option<bool>,
    pub tags: Option<Vec<String>>,
    pub agent_id: Option<String>,
    pub user_id: Option<String>,
    pub repo: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SearchResponse {
    pub results: Vec<MemoryResult>,
    pub total: u64,
    pub query_id: Uuid,
}

#[derive(Debug, Clone, Serialize)]
pub struct MemoryResult {
    pub memory: MemoryUnitDto,
    pub score: f32,
    pub rank: u32,
}

#[derive(Debug, Clone, Serialize)]
pub struct MemoryUnitDto {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub scope: serde_json::Value,
    pub memory_type: String,
    pub scope_visibility: String,
    pub content: String,
    pub importance_score: f32,
    pub decay_score: f32,
    pub pinned: bool,
    pub tags: Vec<String>,
    pub embedding_id: Option<String>,
    pub token_count: Option<i32>,
    pub source_events: Vec<Uuid>,
    pub source_episode_ids: Vec<Uuid>,
    pub corroboration_count: i32,
    pub relevance_score: f64,
    pub promoted_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl From<MemoryUnit> for MemoryUnitDto {
    fn from(unit: MemoryUnit) -> Self {
        let scope = match serde_json::to_value(&unit.scope) {
            Ok(scope) => scope,
            Err(_) => serde_json::Value::Null,
        };

        Self {
            id: unit.id,
            workspace_id: unit.workspace_id,
            scope,
            memory_type: memory_type_as_str(unit.memory_type).to_owned(),
            scope_visibility: scope_visibility_as_str(unit.scope_visibility).to_owned(),
            content: unit.content,
            importance_score: unit.importance_score,
            decay_score: unit.decay_score,
            pinned: unit.pinned,
            tags: unit.tags,
            embedding_id: unit.embedding_id,
            token_count: unit.token_count,
            source_events: unit.source_events,
            source_episode_ids: unit.source_episode_ids,
            corroboration_count: unit.corroboration_count,
            relevance_score: unit.relevance_score,
            promoted_at: unit.promoted_at,
            created_at: unit.created_at,
            updated_at: unit.updated_at,
        }
    }
}

#[derive(Debug, Clone, Deserialize, Validate)]
pub struct ListQuery {
    pub workspace_id: Option<Uuid>,
    #[validate(range(min = 1, max = 100))]
    pub limit: Option<u32>,
    pub offset: Option<u32>,
    pub memory_type: Option<String>,
    pub pinned: Option<bool>,
    pub min_importance: Option<f32>,
    pub agent_id: Option<String>,
    pub user_id: Option<String>,
    pub repo: Option<String>,
    pub source: Option<String>,
    /// Filter to memories derived from a specific source file. Matches the
    /// `source_ref` recorded on linked raw events, ignoring any `#Lstart-Lend`
    /// line anchor (e.g. `src/foo.rs` matches `src/foo.rs#L10-L20`).
    pub source_ref: Option<String>,
    pub as_of: Option<DateTime<Utc>>,
    pub sort: Option<SortField>,
    pub direction: Option<SortDirection>,
}

impl ListQuery {
    pub fn resolved_limit(&self) -> u32 {
        self.limit.unwrap_or(DEFAULT_LIMIT).min(MAX_LIMIT)
    }

    pub fn resolved_offset(&self) -> u32 {
        self.offset.unwrap_or(DEFAULT_OFFSET)
    }

    pub fn resolved_sort(&self) -> SortField {
        self.sort.unwrap_or_default()
    }

    pub fn resolved_direction(&self) -> SortDirection {
        self.direction.unwrap_or_default()
    }
}

#[derive(Debug, Clone, Copy, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SortField {
    #[default]
    ImportanceScore,
    DecayScore,
    RelevanceScore,
    UpdatedAt,
    CreatedAt,
}

#[derive(Debug, Clone, Copy, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum SortDirection {
    Asc,
    #[default]
    Desc,
}

#[derive(Debug, Clone, Serialize)]
pub struct ListResponse {
    pub items: Vec<MemoryUnitDto>,
    pub total: u64,
    pub limit: u32,
    pub offset: u32,
}

#[derive(Debug, Clone, Deserialize, Validate)]
pub struct UpdateMemoryRequest {
    pub pinned: Option<bool>,
    #[validate(range(min = 0.0, max = 1.0))]
    pub importance_score: Option<f32>,
    pub tags: Option<Vec<String>>,
}

impl UpdateMemoryRequest {
    pub fn is_empty(&self) -> bool {
        self.pinned.is_none() && self.importance_score.is_none() && self.tags.is_none()
    }
}

pub fn memory_type_as_str(memory_type: MemoryType) -> &'static str {
    match memory_type {
        MemoryType::Episodic => "episodic",
        MemoryType::Semantic => "semantic",
    }
}

pub fn scope_visibility_as_str(scope_visibility: ScopeVisibility) -> &'static str {
    match scope_visibility {
        ScopeVisibility::Private => "private",
        ScopeVisibility::Workspace => "workspace",
    }
}

pub fn parse_memory_type(value: &str) -> AppResult<MemoryType> {
    match value.to_ascii_lowercase().as_str() {
        "episodic" => Ok(MemoryType::Episodic),
        "semantic" => Ok(MemoryType::Semantic),
        _ => Err(AppError::Validation(
            "memory_type must be one of: episodic, semantic".to_owned(),
        )),
    }
}

pub fn normalized_memory_types(req: &SearchRequest) -> AppResult<Option<Vec<String>>> {
    if let Some(memory_types) = &req.memory_types {
        let normalized = memory_types
            .iter()
            .map(|memory_type| {
                parse_memory_type(memory_type).map(|parsed| memory_type_as_str(parsed).to_owned())
            })
            .collect::<AppResult<Vec<_>>>()?;
        if normalized.is_empty() {
            return Ok(None);
        }
        return Ok(Some(normalized));
    }

    Ok(req
        .filters
        .as_ref()
        .and_then(|filters| filters.memory_type)
        .map(|memory_type| vec![memory_type_as_str(memory_type).to_owned()]))
}

fn default_true() -> bool {
    true
}

fn first_scope_value(values: [Option<&String>; 3]) -> Option<String> {
    values.into_iter().find_map(|value| {
        let trimmed = value?.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_owned())
        }
    })
}

fn normalized_agent_ids(values: &[String]) -> Vec<String> {
    let mut normalized = Vec::new();
    for value in values {
        let trimmed = value.trim();
        if trimmed.is_empty() || normalized.iter().any(|agent_id| agent_id == trimmed) {
            continue;
        }
        normalized.push(trimmed.to_owned());
    }
    normalized
}

pub fn rank_from_index(index: usize) -> u32 {
    match u32::try_from(index.saturating_add(1)) {
        Ok(rank) => rank,
        Err(_) => u32::MAX,
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn search_mode_defaults_to_hybrid() {
        let workspace_id = Uuid::now_v7();
        let request = match serde_json::from_value::<SearchRequest>(json!({
            "query": "memory retrieval",
            "workspace_id": workspace_id
        })) {
            Ok(request) => request,
            Err(error) => panic!("search request should deserialize: {error}"),
        };

        assert_eq!(request.mode, SearchMode::Hybrid);
        assert!(request.include_master_memory);
    }

    #[test]
    fn list_query_sort_defaults_to_importance_desc() {
        let workspace_id = Uuid::now_v7();
        let query = match serde_json::from_value::<ListQuery>(json!({
            "workspace_id": workspace_id
        })) {
            Ok(query) => query,
            Err(error) => panic!("list query should deserialize: {error}"),
        };

        assert_eq!(query.resolved_sort(), SortField::ImportanceScore);
        assert_eq!(query.resolved_direction(), SortDirection::Desc);
        assert_eq!(query.workspace_id, Some(workspace_id));
    }

    #[test]
    fn search_request_resolves_top_level_scope_before_nested_filters() {
        let workspace_id = Uuid::now_v7();
        let request = SearchRequest {
            query: "memory".to_owned(),
            workspace_id,
            mode: SearchMode::Hybrid,
            limit: None,
            offset: None,
            filters: Some(SearchFilters {
                memory_type: None,
                source: None,
                min_importance: None,
                pinned: None,
                tags: None,
                agent_id: Some("filter-agent".to_owned()),
                user_id: None,
                repo: Some("filter/repo".to_owned()),
            }),
            scope: Some(ScopeFilter {
                agent_id: Some("scope-agent".to_owned()),
                user_id: Some("scope-user".to_owned()),
                repo: None,
            }),
            agent_id: Some("top-agent".to_owned()),
            user_id: None,
            repo: None,
            memory_types: None,
            as_of: None,
            include_workspace_pool: false,
            include_master_memory: true,
            inherited_workspace_pool_agent_ids: Vec::new(),
        };

        let scope = match request.resolved_scope_filter() {
            Some(scope) => scope,
            None => panic!("scope should resolve"),
        };

        assert_eq!(scope.agent_id.as_deref(), Some("top-agent"));
        assert_eq!(scope.user_id.as_deref(), Some("scope-user"));
        assert_eq!(scope.repo.as_deref(), Some("filter/repo"));
    }
}
