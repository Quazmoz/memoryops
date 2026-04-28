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
    let hits = keyword_hits(state, req, limit, offset).await?;
    let ids = hits.iter().map(|hit| hit.id).collect::<Vec<_>>();
    let units = store::get_memory_units_by_ids(&state.db, &ids, req.workspace_id).await?;
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
    builder.push(" AND deleted_at IS NULL AND to_tsvector('english', content) @@ plainto_tsquery('english', ");
    builder.push_bind(&req.query);
    builder.push(")");

    if let Some(filters) = &req.filters {
        push_search_filters(&mut builder, filters);
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
    if let Some(memory_type) = filters.memory_type {
        builder.push(" AND memory_type = ");
        builder.push_bind(memory_type);
    }
    if let Some(source) = &filters.source {
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
