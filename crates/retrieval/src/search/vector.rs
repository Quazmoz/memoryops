use std::collections::HashMap;

use common::{
    error::{AppResult, ProviderError},
    AppError, AppState,
};
use processor::embedder::COLLECTION_NAME;
use qdrant_client::qdrant::{
    point_id::PointIdOptions, Condition, Filter, ScoredPoint, SearchPointsBuilder,
};
use uuid::Uuid;

use crate::{
    dto::{
        memory_type_as_str, rank_from_index, MemoryResult, MemoryUnitDto, SearchRequest,
        MIN_SCORE_THRESHOLD,
    },
    store,
};

pub async fn vector_search(
    state: &AppState,
    req: &SearchRequest,
    limit: u32,
) -> AppResult<Vec<MemoryResult>> {
    let embedding = match state.embedding_provider.embed(&req.query).await {
        Ok(embedding) => embedding,
        Err(ProviderError::NotConfigured) => {
            tracing::debug!("embedding provider not configured; skipping vector search");
            return Ok(Vec::new());
        }
        Err(error) => return Err(AppError::Provider(error)),
    };

    let mut conditions = vec![Condition::matches(
        "workspace_id",
        req.workspace_id.to_string(),
    )];
    if let Some(memory_type) = req.filters.as_ref().and_then(|filters| filters.memory_type) {
        conditions.push(Condition::matches(
            "memory_type",
            memory_type_as_str(memory_type).to_owned(),
        ));
    }

    let request = SearchPointsBuilder::new(COLLECTION_NAME, embedding, u64::from(limit))
        .score_threshold(MIN_SCORE_THRESHOLD)
        .filter(Filter::must(conditions));

    let response = match state.qdrant.search_points(request).await {
        Ok(response) => response,
        Err(error) => {
            tracing::warn!(error = ?error, "Qdrant vector search failed; returning empty results");
            return Ok(Vec::new());
        }
    };

    let mut scored_ids = response
        .result
        .into_iter()
        .filter_map(|point| scored_point_uuid(&point).map(|id| (id, point.score.clamp(0.0, 1.0))))
        .collect::<Vec<_>>();
    scored_ids.sort_by(|left, right| right.1.total_cmp(&left.1));

    let ids = scored_ids.iter().map(|(id, _)| *id).collect::<Vec<_>>();
    let units = store::get_memory_units_by_ids(&state.db, &ids, req.workspace_id).await?;
    let mut units_by_id = units
        .into_iter()
        .map(|unit| (unit.id, unit))
        .collect::<HashMap<_, _>>();

    let mut results = Vec::with_capacity(scored_ids.len());
    for (id, score) in scored_ids {
        if let Some(unit) = units_by_id.remove(&id) {
            let rank = rank_from_index(results.len());
            results.push(MemoryResult {
                memory: MemoryUnitDto::from(unit),
                score,
                rank,
            });
        }
    }

    Ok(results)
}

fn scored_point_uuid(point: &ScoredPoint) -> Option<Uuid> {
    match point.id.as_ref()?.point_id_options.as_ref()? {
        PointIdOptions::Uuid(value) => Uuid::parse_str(value).ok(),
        PointIdOptions::Num(_) => None,
    }
}
