use std::collections::HashMap;

use anyhow::anyhow;
use axum::{
    extract::{Path, Query, State},
    Extension, Json,
};
use chrono::{DateTime, Duration, Utc};
use common::{
    auth::AuthContext,
    error::AppResult,
    models::{Entity, MemoryUnit, Source},
    telemetry::{RETRIEVAL_REQUESTS, TOKEN_PACK_BUDGET_USED},
    tokens::estimate_tokens,
    AppError, AppState,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use crate::{
    dto::{MemoryResult, ScopeFilter, SearchMode, SearchRequest, WorkspacePoolAccess, MAX_LIMIT},
    search::{hybrid, keyword, vector},
    services::RetrievalService,
    store,
};

use super::{resolve_workspace_id, workspace_id_param};

const DEFAULT_TRACE_TTL_DAYS: i64 = 30;

fn default_true() -> bool {
    true
}

#[derive(Debug, Deserialize)]
pub struct RetrieveRequest {
    pub query: String,
    pub workspace_id: Uuid,
    pub scope: Option<ScopeFilter>,
    pub agent_id: Option<String>,
    pub user_id: Option<String>,
    pub repo: Option<String>,
    pub token_budget: Option<usize>,
    pub mode: Option<SearchMode>,
    pub include_trace: Option<bool>,
    pub as_of: Option<DateTime<Utc>>,
    #[serde(default)]
    pub include_workspace_pool: bool,
    #[serde(default = "default_true")]
    pub include_master_memory: bool,
}

impl RetrieveRequest {
    fn resolved_scope_filter(&self) -> Option<ScopeFilter> {
        let scope = ScopeFilter {
            agent_id: first_scope_value([
                self.agent_id.as_ref(),
                self.scope
                    .as_ref()
                    .and_then(|scope| scope.agent_id.as_ref()),
            ]),
            user_id: first_scope_value([
                self.user_id.as_ref(),
                self.scope.as_ref().and_then(|scope| scope.user_id.as_ref()),
            ]),
            repo: first_scope_value([
                self.repo.as_ref(),
                self.scope.as_ref().and_then(|scope| scope.repo.as_ref()),
            ]),
        };

        if scope.is_empty() {
            None
        } else {
            Some(scope)
        }
    }
}

#[derive(Debug, Serialize)]
pub struct RetrieveResponse {
    pub query_id: Uuid,
    pub memories: Vec<PackedMemory>,
    pub total_tokens: usize,
    pub trace: Option<RetrievalTrace>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PackedMemory {
    pub id: Uuid,
    pub content: String,
    pub memory_type: String,
    pub importance_score: f32,
    pub decay_score: f32,
    pub relevance_score: f64,
    pub entities: Vec<Entity>,
    pub score_breakdown: ScoreBreakdown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetrievalTrace {
    pub query_id: Uuid,
    pub query: String,
    pub as_of: Option<DateTime<Utc>>,
    pub mode: SearchMode,
    #[serde(default)]
    pub feedback_applied: bool,
    pub candidates_evaluated: usize,
    pub included_count: usize,
    pub excluded_count: usize,
    pub entries: Vec<TraceEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceEntry {
    pub memory_id: Uuid,
    pub score: f32,
    pub included: bool,
    pub exclusion_reason: Option<String>,
    #[serde(default)]
    pub relevance_score: Option<f64>,
    pub score_breakdown: ScoreBreakdown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScoreBreakdown {
    pub semantic_similarity: f32,
    pub keyword_rank: f32,
    pub importance: f32,
    pub recency: f32,
    pub source_authority: f32,
}

#[derive(Debug)]
struct CandidateMemory {
    unit: MemoryUnit,
    score: f32,
    score_breakdown: ScoreBreakdown,
}

#[derive(Debug, sqlx::FromRow)]
struct SourceEventRow {
    id: Uuid,
    source: Source,
}

#[axum::debug_handler]
pub async fn handle_retrieve(
    State(state): State<AppState>,
    auth: Option<Extension<AuthContext>>,
    Json(request): Json<RetrieveRequest>,
) -> AppResult<Json<RetrieveResponse>> {
    let auth_context = auth.as_ref().map(|extension| &extension.0);
    let response = RetrievalService::new(&state)
        .retrieve(auth_context, request)
        .await?;

    Ok(Json(response))
}

pub(crate) async fn execute_retrieve(
    state: &AppState,
    auth_context: Option<&AuthContext>,
    request: RetrieveRequest,
) -> AppResult<RetrieveResponse> {
    if request.query.trim().is_empty() {
        return Err(AppError::Validation("query is required".to_owned()));
    }

    let workspace_id = resolve_workspace_id(auth_context, Some(request.workspace_id))?;
    let mode = request.mode.unwrap_or_default();
    let include_trace = request.include_trace.unwrap_or(false);
    let token_budget = request
        .token_budget
        .unwrap_or(state.config.retrieval.default_token_budget);
    let query_id = Uuid::now_v7();
    let scope_filter = request.resolved_scope_filter();
    let config = super::fetch_workspace_config(&state, workspace_id).await?;
    let mut search_request = SearchRequest {
        query: request.query.clone(),
        workspace_id,
        mode,
        limit: Some(MAX_LIMIT),
        offset: None,
        filters: None,
        scope: scope_filter.clone(),
        agent_id: None,
        user_id: None,
        repo: None,
        memory_types: None,
        as_of: request.as_of,
        include_workspace_pool: request.include_workspace_pool,
        include_master_memory: request.include_master_memory,
        inherited_workspace_pool_agent_ids: Vec::new(),
    };
    search_request.apply_workspace_config(&config);
    let workspace_pool = search_request.workspace_pool_access();

    let search_results = search_candidates(&state, &search_request, mode, &config).await?;
    let candidates = hydrate_candidates(
        &state,
        workspace_id,
        search_results,
        scope_filter.as_ref(),
        request.as_of,
        &workspace_pool,
        mode,
    )
    .await?;
    let packed = pack_memories(candidates, token_budget)?;
    if token_budget > 0 {
        let pct = (packed.total_tokens as f64 / token_budget as f64) * 100.0;
        TOKEN_PACK_BUDGET_USED.record(pct, &[]);
    }
    let trace = RetrievalTrace {
        query_id,
        query: request.query,
        as_of: request.as_of,
        mode,
        feedback_applied: packed.feedback_applied,
        candidates_evaluated: packed.entries.len(),
        included_count: packed.memories.len(),
        excluded_count: packed
            .entries
            .iter()
            .filter(|entry| !entry.included)
            .count(),
        entries: packed.entries,
    };

    persist_trace(&state, workspace_id, &trace).await?;
    RETRIEVAL_REQUESTS.add(1, &[]);

    Ok(RetrieveResponse {
        query_id,
        memories: packed.memories,
        total_tokens: packed.total_tokens,
        trace: if include_trace { Some(trace) } else { None },
    })
}

#[axum::debug_handler]
pub async fn handle_trace_get(
    State(state): State<AppState>,
    auth: Option<Extension<AuthContext>>,
    Path(query_id): Path<Uuid>,
    Query(params): Query<HashMap<String, String>>,
) -> AppResult<Json<RetrievalTrace>> {
    let auth_context = auth.as_ref().map(|extension| &extension.0);
    let workspace_id = resolve_workspace_id(auth_context, workspace_id_param(&params)?)?;
    let trace_value = sqlx::query_scalar::<_, Value>(
        r#"
        SELECT trace
        FROM retrieval_traces
        WHERE query_id = $1
          AND workspace_id = $2
          AND expires_at > now()
        ORDER BY retrieved_at DESC
        LIMIT 1
        "#,
    )
    .bind(query_id)
    .bind(workspace_id)
    .fetch_optional(&state.db)
    .await
    .map_err(AppError::Database)?
    .ok_or_else(|| AppError::NotFound {
        resource: format!("retrieval_trace:{query_id}"),
    })?;
    let trace =
        serde_json::from_value(trace_value).map_err(|error| AppError::Internal(anyhow!(error)))?;

    Ok(Json(trace))
}

async fn search_candidates(
    state: &AppState,
    request: &SearchRequest,
    mode: SearchMode,
    config: &common::models::WorkspaceConfig,
) -> AppResult<Vec<MemoryResult>> {
    match mode {
        SearchMode::Vector => {
            vector::vector_search_results_with_config(state, request, MAX_LIMIT, config).await
        }
        SearchMode::Keyword => keyword::keyword_search(state, request, MAX_LIMIT).await,
        SearchMode::Hybrid => {
            hybrid::hybrid_search_with_config(state, request, MAX_LIMIT, config).await
        }
    }
}

async fn hydrate_candidates(
    state: &AppState,
    workspace_id: Uuid,
    search_results: Vec<MemoryResult>,
    scope: Option<&ScopeFilter>,
    as_of: Option<DateTime<Utc>>,
    workspace_pool: &WorkspacePoolAccess,
    mode: SearchMode,
) -> AppResult<Vec<CandidateMemory>> {
    let ids = search_results
        .iter()
        .map(|result| result.memory.id)
        .collect::<Vec<_>>();
    let units = if let Some(as_of) = as_of {
        store::get_memory_units_by_ids_at(&state.db, &ids, workspace_id, as_of).await?
    } else {
        store::get_memory_units_by_ids(&state.db, &ids, workspace_id).await?
    };
    let mut units_by_id = units
        .into_iter()
        .map(|unit| (unit.id, unit))
        .collect::<HashMap<_, _>>();
    let source_by_event_id = source_by_event_id(&state.db, units_by_id.values()).await?;
    let mut candidates = Vec::with_capacity(search_results.len());

    for result in search_results {
        let Some(unit) = units_by_id.remove(&result.memory.id) else {
            continue;
        };
        if let Some(scope) = scope {
            if !retrieve_scope_matches(&unit, scope, workspace_pool) {
                continue;
            }
        }

        let source_authority = source_authority_for_unit(state, &unit, &source_by_event_id);
        candidates.push(CandidateMemory {
            score: result.score,
            score_breakdown: score_breakdown(&unit, result.score, mode, source_authority),
            unit,
        });
    }

    candidates.sort_by(|left, right| right.score.total_cmp(&left.score));
    Ok(candidates)
}

fn retrieve_scope_matches(
    unit: &MemoryUnit,
    requested_scope: &ScopeFilter,
    workspace_pool: &WorkspacePoolAccess,
) -> bool {
    store::scope_matches_workspace_pool(unit, requested_scope, workspace_pool)
}

struct PackedResult {
    memories: Vec<PackedMemory>,
    total_tokens: usize,
    entries: Vec<TraceEntry>,
    feedback_applied: bool,
}

fn pack_memories(candidates: Vec<CandidateMemory>, token_budget: usize) -> AppResult<PackedResult> {
    let mut memories = Vec::new();
    let mut entries = Vec::with_capacity(candidates.len());
    let mut total_tokens = 0_usize;
    let mut feedback_applied = false;

    for candidate in candidates {
        feedback_applied |= (candidate.unit.relevance_score - 0.5).abs() > f64::EPSILON;
        let estimated_tokens = estimate_tokens(&candidate.unit.content)?;
        if total_tokens.saturating_add(estimated_tokens) > token_budget {
            entries.push(trace_entry(
                &candidate,
                false,
                Some("token_budget_exceeded".to_owned()),
            ));
            continue;
        }

        total_tokens += estimated_tokens;
        entries.push(trace_entry(&candidate, true, None));
        memories.push(PackedMemory {
            id: candidate.unit.id,
            content: candidate.unit.content,
            memory_type: crate::dto::memory_type_as_str(candidate.unit.memory_type).to_owned(),
            importance_score: candidate.unit.importance_score,
            decay_score: candidate.unit.decay_score,
            relevance_score: candidate.unit.relevance_score,
            entities: candidate.unit.entities.0,
            score_breakdown: candidate.score_breakdown,
        });
    }

    Ok(PackedResult {
        memories,
        total_tokens,
        entries,
        feedback_applied,
    })
}

fn trace_entry(
    candidate: &CandidateMemory,
    included: bool,
    exclusion_reason: Option<String>,
) -> TraceEntry {
    TraceEntry {
        memory_id: candidate.unit.id,
        score: candidate.score,
        included,
        exclusion_reason,
        relevance_score: Some(candidate.unit.relevance_score),
        score_breakdown: candidate.score_breakdown.clone(),
    }
}

fn score_breakdown(
    unit: &MemoryUnit,
    score: f32,
    mode: SearchMode,
    source_authority: f32,
) -> ScoreBreakdown {
    ScoreBreakdown {
        semantic_similarity: if mode == SearchMode::Keyword {
            0.0
        } else {
            score
        },
        keyword_rank: if mode == SearchMode::Vector {
            0.0
        } else {
            score
        },
        importance: unit.importance_score,
        recency: unit.decay_score,
        source_authority,
    }
}

async fn source_by_event_id<'a>(
    db: &sqlx::PgPool,
    units: impl Iterator<Item = &'a MemoryUnit>,
) -> AppResult<HashMap<Uuid, Source>> {
    let mut event_ids = units
        .flat_map(|unit| unit.source_events.iter().copied())
        .collect::<Vec<_>>();
    if event_ids.is_empty() {
        return Ok(HashMap::new());
    }
    event_ids.sort_unstable();
    event_ids.dedup();

    let rows =
        sqlx::query_as::<_, SourceEventRow>("SELECT id, source FROM raw_events WHERE id = ANY($1)")
            .bind(event_ids)
            .fetch_all(db)
            .await
            .map_err(AppError::Database)?;

    Ok(rows.into_iter().map(|row| (row.id, row.source)).collect())
}

fn source_authority_for_unit(
    state: &AppState,
    unit: &MemoryUnit,
    source_by_event_id: &HashMap<Uuid, Source>,
) -> f32 {
    let source = unit
        .source_events
        .iter()
        .find_map(|event_id| source_by_event_id.get(event_id).copied());
    source.map_or(0.0, |source| source_authority_weight(state, source))
}

fn source_authority_weight(state: &AppState, source: Source) -> f32 {
    let authority = &state.config.retrieval.source_authority;
    match source {
        Source::GitHub => authority.github,
        Source::Slack => authority.slack,
        Source::Jira => authority.jira,
        Source::Linear => authority.linear,
        Source::Observation => 1.0,
    }
}

fn first_scope_value(values: [Option<&String>; 2]) -> Option<String> {
    values.into_iter().find_map(|value| {
        let trimmed = value?.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_owned())
        }
    })
}

async fn persist_trace(
    state: &AppState,
    workspace_id: Uuid,
    trace: &RetrievalTrace,
) -> AppResult<()> {
    let trace_value =
        serde_json::to_value(trace).map_err(|error| AppError::Internal(anyhow!(error)))?;
    let retention_days = i64::try_from(state.config.telemetry.trace_retention_days)
        .unwrap_or(DEFAULT_TRACE_TTL_DAYS)
        .max(1);
    let expires_at = Utc::now() + Duration::days(retention_days);

    sqlx::query(
        r#"
        INSERT INTO retrieval_traces (id, workspace_id, query_id, trace, expires_at)
        VALUES ($1, $2, $3, $4, $5)
        "#,
    )
    .bind(Uuid::now_v7())
    .bind(workspace_id)
    .bind(trace.query_id)
    .bind(trace_value)
    .bind(expires_at)
    .execute(&state.db)
    .await
    .map(|_| ())
    .map_err(AppError::Database)
}
