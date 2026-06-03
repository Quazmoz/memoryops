use std::{collections::HashMap, sync::Arc};

use chrono::{DateTime, Utc};
use common::{
    build_embedding_provider_for_workspace,
    error::{AppResult, ProviderError},
    models::{MemoryType, WorkspaceConfig},
    providers::EmbeddingProvider,
    AppError, AppState,
};
use qdrant_client::{
    qdrant::{
        point_id::PointIdOptions, Condition, DatetimeRange, Filter, ScoredPoint,
        SearchPointsBuilder, Timestamp,
    },
    Qdrant as QdrantClient,
};
use uuid::Uuid;

use crate::{
    dto::{
        memory_type_as_str, normalized_memory_types, rank_from_index, MemoryResult, MemoryUnitDto,
        ScopeFilter, SearchFilters, SearchRequest, WorkspacePoolAccess, DEFAULT_OFFSET,
        MIN_SCORE_THRESHOLD,
    },
    store,
};

pub(crate) const COLLECTION_NAME: &str = "memoryops_memories";
const VECTOR_CANDIDATE_LIMIT: u64 = 50;

#[derive(Debug, Clone, PartialEq)]
pub struct ScoredCandidate {
    pub memory_id: Uuid,
    pub score: f32,
}

#[derive(Debug, Clone, Copy)]
pub struct VectorSearchOptions<'a> {
    pub workspace_id: Uuid,
    pub query: &'a str,
    pub scope: Option<&'a ScopeFilter>,
    pub memory_types: Option<&'a [String]>,
    pub as_of: Option<DateTime<Utc>>,
    pub workspace_pool: &'a WorkspacePoolAccess,
    pub limit: u64,
}

pub async fn vector_search(
    qdrant: &QdrantClient,
    embedding_provider: &Arc<dyn EmbeddingProvider>,
    options: VectorSearchOptions<'_>,
) -> AppResult<Vec<ScoredCandidate>> {
    let Some(embedding) = query_embedding(embedding_provider, options.query).await? else {
        return Ok(Vec::new());
    };

    vector_search_with_embedding(qdrant, &embedding, options).await
}

async fn query_embedding(
    embedding_provider: &Arc<dyn EmbeddingProvider>,
    query: &str,
) -> AppResult<Option<Vec<f32>>> {
    match embedding_provider.embed(query).await {
        Ok(embedding) => Ok(Some(embedding)),
        Err(ProviderError::NotConfigured) => {
            tracing::warn!("embedding provider not configured; skipping vector search");
            Ok(None)
        }
        Err(error) => Err(AppError::Provider(error)),
    }
}

async fn vector_search_with_embedding(
    qdrant: &QdrantClient,
    embedding: &[f32],
    options: VectorSearchOptions<'_>,
) -> AppResult<Vec<ScoredCandidate>> {
    let request = SearchPointsBuilder::new(COLLECTION_NAME, embedding.to_vec(), options.limit)
        .score_threshold(MIN_SCORE_THRESHOLD)
        .filter(build_vector_filter(
            options.workspace_id,
            options.scope,
            options.memory_types,
            options.as_of,
            options.workspace_pool,
        ));

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
    let workspace_config = crate::handlers::fetch_workspace_config(state, req.workspace_id).await?;
    vector_search_results_with_config(state, req, limit, &workspace_config).await
}

pub(crate) async fn vector_search_results_with_config(
    state: &AppState,
    req: &SearchRequest,
    limit: u32,
    workspace_config: &WorkspaceConfig,
) -> AppResult<Vec<MemoryResult>> {
    let offset = req.offset.unwrap_or(DEFAULT_OFFSET);
    vector_search_results_with_offset_and_config(state, req, limit, offset, workspace_config).await
}

pub(crate) async fn vector_search_results_with_offset_and_config(
    state: &AppState,
    req: &SearchRequest,
    limit: u32,
    offset: u32,
    workspace_config: &WorkspaceConfig,
) -> AppResult<Vec<MemoryResult>> {
    let memory_types = normalized_memory_types(req)?;
    let scope = req.resolved_scope_filter();
    let workspace_pool = req.workspace_pool_access();
    let embedding_provider =
        build_embedding_provider_for_workspace(&state.config, &workspace_config);
    let Some(embedding) = query_embedding(&embedding_provider, &req.query).await? else {
        return Ok(Vec::new());
    };
    let mut candidate_limit = VECTOR_CANDIDATE_LIMIT.max(u64::from(limit.saturating_add(offset)));
    let mut previous_candidate_count = None;

    loop {
        let candidates = vector_search_with_embedding(
            &state.qdrant,
            &embedding,
            VectorSearchOptions {
                workspace_id: req.workspace_id,
                query: &req.query,
                scope: scope.as_ref(),
                memory_types: memory_types.as_deref(),
                as_of: req.as_of,
                workspace_pool: &workspace_pool,
                limit: candidate_limit,
            },
        )
        .await?;

        let candidate_count = candidates.len();
        let results = materialize_vector_results(
            &state.db,
            req,
            scope.as_ref(),
            memory_types.as_deref(),
            &workspace_pool,
            candidates,
            limit,
            offset,
        )
        .await?;

        if results.len() >= limit as usize {
            return Ok(results);
        }
        if candidate_count < candidate_limit as usize
            || previous_candidate_count == Some(candidate_count)
        {
            return Ok(results);
        }

        previous_candidate_count = Some(candidate_count);
        candidate_limit = candidate_limit.saturating_add(VECTOR_CANDIDATE_LIMIT);
    }
}

async fn materialize_vector_results(
    db: &sqlx::PgPool,
    req: &SearchRequest,
    scope: Option<&ScopeFilter>,
    memory_types: Option<&[String]>,
    workspace_pool: &WorkspacePoolAccess,
    candidates: Vec<ScoredCandidate>,
    limit: u32,
    offset: u32,
) -> AppResult<Vec<MemoryResult>> {
    let scored_ids = candidates
        .into_iter()
        .map(|candidate| (candidate.memory_id, candidate.score))
        .collect::<Vec<_>>();
    let ids = scored_ids.iter().map(|(id, _)| *id).collect::<Vec<_>>();
    let units = if let Some(as_of) = req.as_of {
        store::get_memory_units_by_ids_at(db, &ids, req.workspace_id, as_of).await?
    } else {
        store::get_memory_units_by_ids(db, &ids, req.workspace_id).await?
    };
    let mut units_by_id = units
        .into_iter()
        .map(|unit| (unit.id, unit))
        .collect::<HashMap<_, _>>();
    let window_len = usize::try_from(limit.saturating_add(offset)).unwrap_or(usize::MAX);

    let mut matches = Vec::with_capacity(scored_ids.len());
    for (id, score) in scored_ids {
        if let Some(unit) = units_by_id.remove(&id) {
            if let Some(scope) = scope {
                if !store::scope_matches_workspace_pool(&unit, scope, workspace_pool) {
                    continue;
                }
            }
            if !matches_memory_type(unit.memory_type, memory_types) {
                continue;
            }
            if !non_type_search_filters_match(
                unit.scope.source.as_deref(),
                unit.importance_score,
                unit.pinned,
                &unit.tags,
                req.filters.as_ref(),
            ) {
                continue;
            }
            matches.push(MemoryResult {
                memory: MemoryUnitDto::from(unit),
                score,
                rank: 0,
            });
            if matches.len() >= window_len {
                break;
            }
        }
    }

    let skip = offset as usize;
    if skip >= matches.len() {
        return Ok(Vec::new());
    }

    Ok(matches
        .into_iter()
        .skip(skip)
        .take(limit as usize)
        .enumerate()
        .map(|(index, mut result)| {
            result.rank = rank_from_index(index);
            result
        })
        .collect())
}

pub fn build_vector_filter(
    workspace_id: Uuid,
    scope: Option<&ScopeFilter>,
    memory_types: Option<&[String]>,
    as_of: Option<DateTime<Utc>>,
    workspace_pool: &crate::dto::WorkspacePoolAccess,
) -> Filter {
    let mut conditions = vec![Condition::matches("workspace_id", workspace_id.to_string())];
    if let Some(memory_types) = memory_types {
        if !memory_types.is_empty() {
            conditions.push(Condition::matches("memory_type", memory_types.to_vec()));
        }
    }
    if let Some(as_of) = as_of {
        conditions.push(Condition::datetime_range(
            "created_at",
            DatetimeRange {
                lte: Some(timestamp_from_datetime(as_of)),
                ..Default::default()
            },
        ));
    }
    if let Some(scope) = scope {
        if let Some(agent_id) = &scope.agent_id {
            conditions.push(agent_scope_condition(agent_id, workspace_pool));
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

fn agent_scope_condition(
    agent_id: &str,
    workspace_pool: &crate::dto::WorkspacePoolAccess,
) -> Condition {
    if workspace_pool.include_all_workspace {
        return Filter::should([
            Condition::matches("agent_id", agent_id.to_owned()),
            Condition::matches("scope_visibility", "workspace".to_owned()),
        ])
        .into();
    }

    if !workspace_pool.inherited_agent_ids.is_empty() {
        return Filter::should([
            Condition::matches("agent_id", agent_id.to_owned()),
            Filter::must([
                Condition::matches("scope_visibility", "workspace".to_owned()),
                Condition::matches("agent_id", workspace_pool.inherited_agent_ids.clone()),
            ])
            .into(),
        ])
        .into();
    }

    Condition::matches("agent_id", agent_id.to_owned())
}

fn timestamp_from_datetime(value: DateTime<Utc>) -> Timestamp {
    Timestamp {
        seconds: value.timestamp(),
        nanos: value.timestamp_subsec_nanos() as i32,
    }
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

fn source_matches(memory_source: Option<&str>, source_filter: Option<&str>) -> bool {
    source_filter.is_none_or(|expected| memory_source == Some(expected))
}

fn non_type_search_filters_match(
    memory_source: Option<&str>,
    importance_score: f32,
    pinned: bool,
    memory_tags: &[String],
    filters: Option<&SearchFilters>,
) -> bool {
    let Some(filters) = filters else {
        return true;
    };

    let source_filter = filters
        .source
        .as_deref()
        .map(str::trim)
        .filter(|source| !source.is_empty());
    if !source_matches(memory_source, source_filter) {
        return false;
    }

    if let Some(min_importance) = filters.min_importance {
        if importance_score < min_importance {
            return false;
        }
    }

    if let Some(expected_pinned) = filters.pinned {
        if pinned != expected_pinned {
            return false;
        }
    }

    if let Some(tags) = &filters.tags {
        if !tags.iter().all(|tag| memory_tags.contains(tag)) {
            return false;
        }
    }

    true
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

        let filter = build_vector_filter(
            workspace_id,
            Some(&scope),
            None,
            None,
            &crate::dto::WorkspacePoolAccess::default(),
        );
        let debug = format!("{filter:?}");

        assert!(debug.contains("workspace_id"));
        assert!(debug.contains(&workspace_id.to_string()));
        assert!(debug.contains("agent_id"));
        assert!(debug.contains("repo"));
    }

    #[test]
    fn source_match_requires_matching_source_when_filter_is_present() {
        assert!(source_matches(Some("github"), Some("github")));
        assert!(!source_matches(Some("slack"), Some("github")));
        assert!(!source_matches(None, Some("github")));
        assert!(source_matches(Some("slack"), None));
    }

    #[test]
    fn non_type_search_filters_require_importance_pin_and_tags() {
        let filters = SearchFilters {
            memory_type: None,
            source: Some("github".to_owned()),
            min_importance: Some(0.7),
            pinned: Some(true),
            tags: Some(vec!["rust".to_owned(), "api".to_owned()]),
            agent_id: None,
            user_id: None,
            repo: None,
        };
        let tags = vec!["rust".to_owned(), "api".to_owned(), "security".to_owned()];

        assert!(non_type_search_filters_match(
            Some("github"),
            0.8,
            true,
            &tags,
            Some(&filters),
        ));
        assert!(!non_type_search_filters_match(
            Some("github"),
            0.6,
            true,
            &tags,
            Some(&filters),
        ));
        assert!(!non_type_search_filters_match(
            Some("github"),
            0.8,
            false,
            &tags,
            Some(&filters),
        ));
        assert!(!non_type_search_filters_match(
            Some("github"),
            0.8,
            true,
            &["rust".to_owned()],
            Some(&filters),
        ));
    }

    #[test]
    fn vector_filter_includes_memory_types_when_supplied() {
        let workspace_id = Uuid::now_v7();
        let memory_types = vec!["semantic".to_owned()];

        let filter = build_vector_filter(
            workspace_id,
            None,
            Some(&memory_types),
            None,
            &crate::dto::WorkspacePoolAccess::default(),
        );
        let debug = format!("{filter:?}");

        assert!(debug.contains("memory_type"));
        assert!(debug.contains("semantic"));
    }

    #[test]
    fn vector_filter_includes_as_of_datetime_range() {
        let workspace_id = Uuid::now_v7();
        let filter = build_vector_filter(
            workspace_id,
            None,
            None,
            Some(Utc::now()),
            &crate::dto::WorkspacePoolAccess::default(),
        );
        let debug = format!("{filter:?}");

        assert!(debug.contains("created_at"));
    }
}
