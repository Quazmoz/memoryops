use std::collections::HashMap;

use anyhow::anyhow;
use axum::{
    extract::{Path, Query, State},
    Extension, Json,
};
use chrono::{Duration, Utc};
use common::{
    auth::AuthContext,
    error::AppResult,
    models::{Entity, MemoryScope, MemoryUnit},
    AppError, AppState,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use crate::{
    dto::{MemoryResult, SearchMode, SearchRequest, MAX_LIMIT},
    search::{hybrid, keyword, vector},
    store,
};

use super::{resolve_workspace_id, workspace_id_param};

const DEFAULT_TRACE_TTL_DAYS: i64 = 30;

#[derive(Debug, Deserialize)]
pub struct RetrieveRequest {
    pub query: String,
    pub workspace_id: Uuid,
    pub scope: Option<MemoryScope>,
    pub token_budget: Option<usize>,
    pub mode: Option<SearchMode>,
    pub include_trace: Option<bool>,
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
    pub entities: Vec<Entity>,
    pub score_breakdown: ScoreBreakdown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetrievalTrace {
    pub query_id: Uuid,
    pub query: String,
    pub mode: SearchMode,
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

#[axum::debug_handler]
pub async fn handle_retrieve(
    State(state): State<AppState>,
    auth: Option<Extension<AuthContext>>,
    Json(request): Json<RetrieveRequest>,
) -> AppResult<Json<RetrieveResponse>> {
    if request.query.trim().is_empty() {
        return Err(AppError::Validation("query is required".to_owned()));
    }

    let auth_context = auth.as_ref().map(|extension| &extension.0);
    let workspace_id = resolve_workspace_id(auth_context, Some(request.workspace_id))?;
    let mode = request.mode.unwrap_or_default();
    let include_trace = request.include_trace.unwrap_or(false);
    let token_budget = request
        .token_budget
        .unwrap_or(state.config.retrieval.default_token_budget);
    let query_id = Uuid::now_v7();
    let search_request = SearchRequest {
        query: request.query.clone(),
        workspace_id,
        mode,
        limit: Some(MAX_LIMIT),
        offset: None,
        filters: None,
    };

    let search_results = search_candidates(&state, &search_request, mode).await?;
    let candidates = hydrate_candidates(
        &state,
        workspace_id,
        search_results,
        request.scope.as_ref(),
        mode,
    )
    .await?;
    let packed = pack_memories(candidates, token_budget);
    let trace = RetrievalTrace {
        query_id,
        query: request.query,
        mode,
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

    Ok(Json(RetrieveResponse {
        query_id,
        memories: packed.memories,
        total_tokens: packed.total_tokens,
        trace: if include_trace {
            Some(trace)
        } else {
            None
        },
    }))
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
) -> AppResult<Vec<MemoryResult>> {
    match mode {
        SearchMode::Vector => vector::vector_search(state, request, MAX_LIMIT).await,
        SearchMode::Keyword => keyword::keyword_search(state, request, MAX_LIMIT).await,
        SearchMode::Hybrid => hybrid::hybrid_search(state, request, MAX_LIMIT).await,
    }
}

async fn hydrate_candidates(
    state: &AppState,
    workspace_id: Uuid,
    search_results: Vec<MemoryResult>,
    scope: Option<&MemoryScope>,
    mode: SearchMode,
) -> AppResult<Vec<CandidateMemory>> {
    let ids = search_results
        .iter()
        .map(|result| result.memory.id)
        .collect::<Vec<_>>();
    let units = store::get_memory_units_by_ids(&state.db, &ids, workspace_id).await?;
    let mut units_by_id = units
        .into_iter()
        .map(|unit| (unit.id, unit))
        .collect::<HashMap<_, _>>();
    let mut candidates = Vec::with_capacity(search_results.len());

    for result in search_results {
        let Some(unit) = units_by_id.remove(&result.memory.id) else {
            continue;
        };
        if let Some(scope) = scope {
            if !scope_matches(&unit.scope, scope) {
                continue;
            }
        }

        candidates.push(CandidateMemory {
            score: result.score,
            score_breakdown: score_breakdown(&unit, result.score, mode),
            unit,
        });
    }

    candidates.sort_by(|left, right| right.score.total_cmp(&left.score));
    Ok(candidates)
}

struct PackedResult {
    memories: Vec<PackedMemory>,
    total_tokens: usize,
    entries: Vec<TraceEntry>,
}

fn pack_memories(candidates: Vec<CandidateMemory>, token_budget: usize) -> PackedResult {
    let mut memories = Vec::new();
    let mut entries = Vec::with_capacity(candidates.len());
    let mut total_tokens = 0_usize;

    for candidate in candidates {
        let estimated_tokens = estimate_tokens(&candidate.unit.content);
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
            entities: candidate.unit.entities.0,
            score_breakdown: candidate.score_breakdown,
        });
    }

    PackedResult {
        memories,
        total_tokens,
        entries,
    }
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
        score_breakdown: candidate.score_breakdown.clone(),
    }
}

fn score_breakdown(unit: &MemoryUnit, score: f32, mode: SearchMode) -> ScoreBreakdown {
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
        source_authority: 0.0,
    }
}

fn scope_matches(unit_scope: &MemoryScope, requested_scope: &MemoryScope) -> bool {
    if let Some(agent_id) = &requested_scope.agent_id {
        if unit_scope.agent_id.as_ref() != Some(agent_id) {
            return false;
        }
    }
    if let Some(user_id) = &requested_scope.user_id {
        if unit_scope.user_id.as_ref() != Some(user_id) {
            return false;
        }
    }
    if let Some(repo) = &requested_scope.repo {
        if unit_scope.repo.as_ref() != Some(repo) {
            return false;
        }
    }

    true
}

fn estimate_tokens(content: &str) -> usize {
    (content.len() / 4).max(1)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_estimate_uses_char_division_floor_with_minimum_one() {
        assert_eq!(estimate_tokens("abc"), 1);
        assert_eq!(estimate_tokens("abcdefgh"), 2);
    }
}
