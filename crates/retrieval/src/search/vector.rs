use std::{collections::HashMap, sync::Arc};

use common::{
    error::{AppResult, ProviderError},
    models::MemoryType,
    providers::EmbeddingProvider,
    AppError, AppState,
};
use qdrant_client::{
    qdrant::{point_id::PointIdOptions, Condition, Filter, ScoredPoint, SearchPointsBuilder},
    Qdrant as QdrantClient,
};
use uuid::Uuid;

use crate::{
    dto::{
        memory_type_as_str, normalized_memory_types, rank_from_index, MemoryResult, MemoryUnitDto,
        ScopeFilter, SearchRequest, MIN_SCORE_THRESHOLD,
    },
    store,
};

const COLLECTION_NAME: &str = "memoryops_memories";
const VECTOR_CANDIDATE_LIMIT: u64 = 50;

#[derive(Debug, Clone, PartialEq)]
pub struct ScoredCandidate {
    pub memory_id: Uuid,
    pub score: f32,
}

pub async fn vector_search(
    qdrant: &QdrantClient,
    embedding_provider: &Arc<dyn EmbeddingProvider>,
    workspace_id: Uuid,
    query: &str,
    scope: Option<&ScopeFilter>,
    memory_types: Option<&[String]>,
    limit: u64,
) -> AppResult<Vec<ScoredCandidate>> {
    let embedding = match embedding_provider.embed(query).await {
        Ok(embedding) => embedding,
        Err(ProviderError::NotConfigured) => {
            tracing::warn!("embedding provider not configured; skipping vector search");
            return Ok(Vec::new());
        }
        Err(error) => return Err(AppError::Provider(error)),
    };

    let request = SearchPointsBuilder::new(COLLECTION_NAME, embedding, limit)
        .score_threshold(MIN_SCORE_THRESHOLD)
        .filter(build_vector_filter(workspace_id, scope, memory_types));

    let response = match qdrant.search_points(request).await {
        Ok(response) => response,
        Err(error) => {
            tracing::warn!(error = ?error, "Qdrant vector search failed; returning empty results");
            return Ok(Vec::new());
        }
    };

    let mut candidates = response
        .result
        .into_iter()
        .filter_map(|point| {
            scored_point_uuid(&point).map(|memory_id| ScoredCandidate {
                memory_id,
                score: point.score.clamp(0.0, 1.0),
            })
        })
        .collect::<Vec<_>>();
    candidates.sort_by(|left, right| right.score.total_cmp(&left.score));

    Ok(candidates)
}

pub async fn vector_search_results(
    state: &AppState,
    req: &SearchRequest,
    limit: u32,
) -> AppResult<Vec<MemoryResult>> {
    let memory_types = normalized_memory_types(req)?;
    let scope = req.resolved_scope_filter();
    let candidates = vector_search(
        &state.qdrant,
        &state.embedding_provider,
        req.workspace_id,
        &req.query,
        scope.as_ref(),
        memory_types.as_deref(),
        VECTOR_CANDIDATE_LIMIT.max(u64::from(limit)),
    )
    .await?;

    let scored_ids = candidates
        .into_iter()
        .map(|candidate| (candidate.memory_id, candidate.score))
        .collect::<Vec<_>>();
    let ids = scored_ids.iter().map(|(id, _)| *id).collect::<Vec<_>>();
    let units = store::get_memory_units_by_ids(&state.db, &ids, req.workspace_id).await?;
    let mut units_by_id = units
        .into_iter()
        .map(|unit| (unit.id, unit))
        .collect::<HashMap<_, _>>();

    let mut results = Vec::with_capacity(scored_ids.len());
    for (id, score) in scored_ids {
        if let Some(unit) = units_by_id.remove(&id) {
            if !matches_memory_type(unit.memory_type, memory_types.as_deref()) {
                continue;
            }
            let rank = rank_from_index(results.len());
            results.push(MemoryResult {
                memory: MemoryUnitDto::from(unit),
                score,
                rank,
            });
            if results.len() >= limit as usize {
                break;
            }
        }
    }

    Ok(results)
}

pub fn build_vector_filter(
    workspace_id: Uuid,
    scope: Option<&ScopeFilter>,
    memory_types: Option<&[String]>,
) -> Filter {
    let mut conditions = vec![Condition::matches("workspace_id", workspace_id.to_string())];
    if let Some(memory_types) = memory_types {
        if !memory_types.is_empty() {
            conditions.push(Condition::matches("memory_type", memory_types.to_vec()));
        }
    }
    if let Some(scope) = scope {
        if let Some(agent_id) = &scope.agent_id {
            conditions.push(Condition::matches("agent_id", agent_id.clone()));
        }
        if let Some(user_id) = &scope.user_id {
            conditions.push(Condition::matches("user_id", user_id.clone()));
        }
        if let Some(repo) = &scope.repo {
            conditions.push(Condition::matches("repo", repo.clone()));
        }
    }

    Filter::must(conditions)
}

fn scored_point_uuid(point: &ScoredPoint) -> Option<Uuid> {
    match point.id.as_ref()?.point_id_options.as_ref()? {
        PointIdOptions::Uuid(value) => Uuid::parse_str(value).ok(),
        PointIdOptions::Num(_) => None,
    }
}

fn matches_memory_type(memory_type: MemoryType, filters: Option<&[String]>) -> bool {
    filters.is_none_or(|filters| {
        filters
            .iter()
            .any(|filter| filter == memory_type_as_str(memory_type))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vector_filter_includes_workspace_id() {
        let workspace_id = Uuid::now_v7();
        let scope = ScopeFilter {
            agent_id: Some("agent".to_owned()),
            user_id: Some("user".to_owned()),
            repo: Some("Quazmoz/memoryops".to_owned()),
        };

        let filter = build_vector_filter(workspace_id, Some(&scope), None);
        let debug = format!("{filter:?}");

        assert!(debug.contains("workspace_id"));
        assert!(debug.contains(&workspace_id.to_string()));
        assert!(debug.contains("agent_id"));
        assert!(debug.contains("repo"));
    }

    #[test]
    fn vector_filter_includes_memory_types_when_supplied() {
        let workspace_id = Uuid::now_v7();
        let memory_types = vec!["semantic".to_owned()];

        let filter = build_vector_filter(workspace_id, None, Some(&memory_types));
        let debug = format!("{filter:?}");

        assert!(debug.contains("memory_type"));
        assert!(debug.contains("semantic"));
    }
}
