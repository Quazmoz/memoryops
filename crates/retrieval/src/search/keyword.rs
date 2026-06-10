use common::{error::AppResult, AppState};
use sqlx::{Postgres, QueryBuilder};
use uuid::Uuid;

use crate::{
    dto::{
        normalized_memory_types, rank_from_index, MemoryResult, MemoryUnitDto, SearchFilters,
        SearchRequest, DEFAULT_OFFSET,
    },
    store,
};

#[derive(Debug, sqlx::FromRow)]
struct KeywordHit {
    id: Uuid,
    rank_score: f32,
}

pub async fn keyword_search(
    state: &AppState,
    req: &SearchRequest,
    limit: u32,
) -> AppResult<Vec<MemoryResult>> {
    let offset = req.offset.unwrap_or(DEFAULT_OFFSET);
    keyword_search_with_offset(state, req, limit, offset).await
}

pub(crate) async fn keyword_search_with_offset(
    state: &AppState,
    req: &SearchRequest,
    limit: u32,
    offset: u32,
) -> AppResult<Vec<MemoryResult>> {
    let hits = keyword_hits(state, req, limit, offset).await?;
    let ids = hits.iter().map(|hit| hit.id).collect::<Vec<_>>();
    let units = if let Some(as_of) = req.as_of {
        store::get_memory_units_by_ids_at(&state.db, &ids, req.workspace_id, as_of).await?
    } else {
        store::get_memory_units_by_ids(&state.db, &ids, req.workspace_id).await?
    };
    let mut units_by_id = units
        .into_iter()
        .map(|unit| (unit.id, unit))
        .collect::<std::collections::HashMap<_, _>>();

    let mut results = Vec::with_capacity(hits.len());
    for hit in hits {
        if let Some(unit) = units_by_id.remove(&hit.id) {
            let rank = rank_from_index(results.len());
            results.push(MemoryResult {
                memory: MemoryUnitDto::from(unit),
                score: hit.rank_score.clamp(0.0, 1.0),
                rank,
            });
        }
    }

    Ok(results)
}

async fn keyword_hits(
    state: &AppState,
    req: &SearchRequest,
    limit: u32,
    offset: u32,
) -> AppResult<Vec<KeywordHit>> {
    let mut builder = QueryBuilder::<Postgres>::new(
        "SELECT id, ts_rank(to_tsvector('english', content), plainto_tsquery('english', ",
    );
    builder.push_bind(&req.query);
    builder.push(")) AS rank_score FROM memory_units WHERE workspace_id = ");
    builder.push_bind(req.workspace_id);
    if let Some(as_of) = req.as_of {
        builder.push(" AND created_at <= ");
        builder.push_bind(as_of);
        builder.push(" AND (deleted_at IS NULL OR deleted_at > ");
        builder.push_bind(as_of);
        builder.push(")");
    } else {
        builder.push(" AND deleted_at IS NULL");
    }
    builder.push(" AND to_tsvector('english', content) @@ plainto_tsquery('english', ");
    builder.push_bind(&req.query);
    builder.push(")");

    if let Some(filters) = &req.filters {
        push_search_filters(&mut builder, filters);
    }
    if let Some(scope) = req.resolved_scope_filter() {
        store::push_scope_filter(&mut builder, &scope, &req.workspace_pool_access(), None);
    }
    if let Some(memory_types) = normalized_memory_types(req)? {
        builder.push(" AND memory_type::text = ANY(");
        builder.push_bind(memory_types);
        builder.push(")");
    }

    builder.push(" ORDER BY rank_score DESC LIMIT ");
    builder.push_bind(i64::from(limit));
    builder.push(" OFFSET ");
    builder.push_bind(i64::from(offset));

    builder
        .build_query_as::<KeywordHit>()
        .fetch_all(&state.db)
        .await
        .map_err(common::AppError::Database)
}

fn push_search_filters<'a>(builder: &mut QueryBuilder<'a, Postgres>, filters: &'a SearchFilters) {
    if let Some(source) = filters
        .source
        .as_deref()
        .map(str::trim)
        .filter(|source| !source.is_empty())
    {
        builder.push(" AND scope->>'source' = ");
        builder.push_bind(source);
    }
    if let Some(min_importance) = filters.min_importance {
        builder.push(" AND importance_score >= ");
        builder.push_bind(min_importance);
    }
    if let Some(pinned) = filters.pinned {
        builder.push(" AND pinned = ");
        builder.push_bind(pinned);
    }
    if let Some(tags) = &filters.tags {
        if !tags.is_empty() {
            builder.push(" AND tags @> ");
            builder.push_bind(tags.clone());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::dto::ScopeFilter;

    #[test]
    fn scope_filter_with_all_fields_adds_where_clauses() {
        let mut builder = QueryBuilder::<Postgres>::new(
            "SELECT id, 1.0 AS rank_score FROM memory_units WHERE workspace_id = ",
        );
        builder.push_bind(Uuid::now_v7());
        let scope = ScopeFilter {
            agent_id: Some("agent-1".to_owned()),
            user_id: Some("user-1".to_owned()),
            repo: Some("Quazmoz/memoryops".to_owned()),
        };

        store::push_scope_filter(
            &mut builder,
            &scope,
            &crate::dto::WorkspacePoolAccess::default(),
            None,
        );
        let sql = builder.sql();

        assert!(sql.contains("agent_id"));
        assert!(sql.contains("user_id"));
        assert!(sql.contains("repo"));
        assert!(sql.contains(" IS NULL"));
        assert!(!sql.contains("scope_visibility"));
    }

    #[test]
    fn empty_scope_filter_adds_no_where_clauses() {
        let mut builder = QueryBuilder::<Postgres>::new(
            "SELECT id, 1.0 AS rank_score FROM memory_units WHERE workspace_id = ",
        );
        builder.push_bind(Uuid::now_v7());
        let before = builder.sql().to_owned();

        store::push_scope_filter(
            &mut builder,
            &ScopeFilter::default(),
            &crate::dto::WorkspacePoolAccess::default(),
            None,
        );

        assert_eq!(builder.sql(), before);
    }

    #[test]
    fn scope_filter_includes_master_memory_when_enabled() {
        let mut builder = QueryBuilder::<Postgres>::new(
            "SELECT id, 1.0 AS rank_score FROM memory_units WHERE workspace_id = ",
        );
        builder.push_bind(Uuid::now_v7());
        let scope = ScopeFilter {
            agent_id: Some("agent-1".to_owned()),
            user_id: Some("user-1".to_owned()),
            repo: Some("Quazmoz/memoryops".to_owned()),
        };

        store::push_scope_filter(
            &mut builder,
            &scope,
            &crate::dto::WorkspacePoolAccess {
                include_all_workspace: false,
                include_master_memory: true,
                inherited_agent_ids: Vec::new(),
            },
            None,
        );
        let sql = builder.sql();

        assert!(sql.contains("scope_visibility = 'workspace'"));
        assert!(sql.contains("agent_id IS NULL"));
        assert!(sql.contains("user_id IS NULL"));
        assert!(sql.contains("repo IS NULL OR repo ="));
    }

    #[test]
    fn scope_filter_excludes_master_memory_when_disabled() {
        let mut builder = QueryBuilder::<Postgres>::new(
            "SELECT id, 1.0 AS rank_score FROM memory_units WHERE workspace_id = ",
        );
        builder.push_bind(Uuid::now_v7());
        let scope = ScopeFilter {
            agent_id: Some("agent-1".to_owned()),
            user_id: Some("user-1".to_owned()),
            repo: Some("Quazmoz/memoryops".to_owned()),
        };

        store::push_scope_filter(
            &mut builder,
            &scope,
            &crate::dto::WorkspacePoolAccess::default(),
            None,
        );

        assert!(!builder.sql().contains("scope_visibility = 'workspace'"));
    }

    #[test]
    fn search_filters_leave_memory_type_to_normalized_filtering() {
        let mut builder = QueryBuilder::<Postgres>::new("SELECT id FROM memory_units WHERE true");
        push_search_filters(
            &mut builder,
            &SearchFilters {
                memory_type: Some(common::models::MemoryType::Semantic),
                source: None,
                min_importance: None,
                pinned: None,
                tags: None,
                agent_id: None,
                user_id: None,
                repo: None,
            },
        );

        assert!(!builder.sql().contains("memory_type ="));
    }
}
