use std::collections::HashMap;

use common::{error::AppResult, models::WorkspaceConfig, AppState};
use uuid::Uuid;

use crate::dto::{rank_from_index, MemoryResult, SearchRequest, DEFAULT_OFFSET};

use super::{
    keyword::keyword_search_with_offset, vector::vector_search_results_with_offset_and_config,
};

pub const RRF_K: f32 = 60.0;
pub const RELEVANCE_SCORE_WEIGHT: f32 = 0.10;

#[derive(Debug, Clone, PartialEq)]
pub struct FusedRank {
    pub id: Uuid,
    pub raw_score: f32,
    pub score: f32,
}

pub async fn hybrid_search(
    state: &AppState,
    req: &SearchRequest,
    limit: u32,
) -> AppResult<Vec<MemoryResult>> {
    let workspace_config = crate::handlers::fetch_workspace_config(state, req.workspace_id).await?;
    hybrid_search_with_config(state, req, limit, &workspace_config).await
}

pub async fn hybrid_search_with_config(
    state: &AppState,
    req: &SearchRequest,
    limit: u32,
    workspace_config: &WorkspaceConfig,
) -> AppResult<Vec<MemoryResult>> {
    let offset = req.offset.unwrap_or(DEFAULT_OFFSET);
    let requested = limit.saturating_add(offset);
    let candidate_limit = requested.saturating_mul(2).max(1);
    let (vector_results, keyword_results) = tokio::join!(
        vector_search_results_with_offset_and_config(
            state,
            req,
            candidate_limit,
            0,
            workspace_config
        ),
        keyword_search_with_offset(state, req, candidate_limit, 0)
    );

    let vector_results = vector_results?;
    let keyword_results = keyword_results?;

    if vector_results.is_empty() {
        return Ok(paginate_results_owned(
            keyword_results,
            limit as usize,
            offset as usize,
        ));
    }

    let vector_ids = vector_results
        .iter()
        .map(|result| result.memory.id)
        .collect::<Vec<_>>();
    let keyword_ids = keyword_results
        .iter()
        .map(|result| result.memory.id)
        .collect::<Vec<_>>();
    let fused = fuse_ranked_ids(&vector_ids, &keyword_ids, candidate_limit as usize);
    let results = materialize_fused_results(fused, vector_results, keyword_results);

    Ok(paginate_results_owned(
        results,
        limit as usize,
        offset as usize,
    ))
}

fn materialize_fused_results(
    fused: Vec<FusedRank>,
    vector_results: Vec<MemoryResult>,
    keyword_results: Vec<MemoryResult>,
) -> Vec<MemoryResult> {
    let mut results_by_id = HashMap::with_capacity(vector_results.len() + keyword_results.len());
    for result in vector_results.into_iter().chain(keyword_results) {
        results_by_id.entry(result.memory.id).or_insert(result);
    }

    let mut results = Vec::with_capacity(fused.len());
    for fused_rank in fused {
        if let Some(mut result) = results_by_id.remove(&fused_rank.id) {
            result.score = apply_relevance_score(fused_rank.score, result.memory.relevance_score);
            result.rank = 0;
            results.push(result);
        }
    }

    results.sort_by(|left, right| {
        right
            .score
            .total_cmp(&left.score)
            .then_with(|| left.memory.id.as_u128().cmp(&right.memory.id.as_u128()))
    });

    results
}

pub fn apply_relevance_score(rrf_score: f32, relevance_score: f64) -> f32 {
    let relevance_delta = relevance_score as f32 - 0.5;
    rrf_score * (1.0 + RELEVANCE_SCORE_WEIGHT * relevance_delta)
}

pub fn rrf_score(rank: u32) -> f32 {
    1.0 / (RRF_K + rank as f32)
}

pub fn fuse_ranked_ids(vector_ids: &[Uuid], keyword_ids: &[Uuid], limit: usize) -> Vec<FusedRank> {
    let mut scores = HashMap::<Uuid, f32>::with_capacity(vector_ids.len() + keyword_ids.len());
    add_rrf_scores(&mut scores, vector_ids);
    add_rrf_scores(&mut scores, keyword_ids);

    let mut fused = scores
        .into_iter()
        .map(|(id, raw_score)| FusedRank {
            id,
            raw_score,
            score: raw_score,
        })
        .collect::<Vec<_>>();
    fused.sort_by(|left, right| {
        right
            .raw_score
            .total_cmp(&left.raw_score)
            .then_with(|| left.id.as_u128().cmp(&right.id.as_u128()))
    });
    fused.truncate(limit);

    let max_score = fused
        .iter()
        .map(|rank| rank.raw_score)
        .fold(0.0_f32, f32::max);
    if max_score > 0.0 {
        for rank in &mut fused {
            rank.score = (rank.raw_score / max_score).clamp(0.0, 1.0);
        }
    }

    fused
}

pub fn paginate_results(
    results: &[MemoryResult],
    limit: usize,
    offset: usize,
) -> Vec<MemoryResult> {
    paginate_results_owned(results.to_vec(), limit, offset)
}

fn paginate_results_owned(
    results: Vec<MemoryResult>,
    limit: usize,
    offset: usize,
) -> Vec<MemoryResult> {
    results
        .into_iter()
        .skip(offset)
        .take(limit)
        .enumerate()
        .map(|(index, mut result)| {
            result.rank = rank_from_index(index);
            result
        })
        .collect()
}

pub fn keyword_fallback_results(results: &[MemoryResult], limit: usize) -> Vec<MemoryResult> {
    paginate_results(results, limit, 0)
}

fn add_rrf_scores(scores: &mut HashMap<Uuid, f32>, ids: &[Uuid]) {
    for (index, id) in ids.iter().enumerate() {
        let rank = rank_from_index(index);
        let entry = scores.entry(*id).or_insert(0.0);
        *entry += rrf_score(rank);
    }
}

#[cfg(test)]
mod tests {
    use chrono::Utc;
    use serde_json::json;

    use crate::dto::MemoryUnitDto;

    use super::*;

    #[test]
    fn rrf_fusion_scores_correctly() {
        let first = Uuid::from_u128(1);
        let second = Uuid::from_u128(2);
        let third = Uuid::from_u128(3);

        let fused = fuse_ranked_ids(&[first, second], &[second, third], 3);

        assert_eq!(fused[0].id, second);
        assert_eq!(fused[1].id, first);
        assert_eq!(fused[2].id, third);
        assert!(fused.iter().all(|rank| rank.score <= 1.0));
    }

    #[test]
    fn rrf_uses_k60() {
        let id = Uuid::from_u128(1);
        let fused = fuse_ranked_ids(&[id], &[id], 1);
        let expected = 2.0 / 61.0;

        assert!((fused[0].raw_score - expected).abs() < 0.0001);
    }

    #[test]
    fn relevance_score_adjusts_rank() {
        let base = 0.75;
        let boosted = apply_relevance_score(base, 1.0);
        let penalized = apply_relevance_score(base, 0.0);

        assert!(boosted > base);
        assert!(penalized < base);
    }

    #[test]
    fn paginate_results_assigns_ranks_after_offset() {
        let workspace_id = Uuid::now_v7();
        let make_result = |id: Uuid, rank: u32| MemoryResult {
            memory: MemoryUnitDto {
                id,
                workspace_id,
                scope: json!({}),
                memory_type: "episodic".to_owned(),
                scope_visibility: "private".to_owned(),
                content: format!("memory-{rank}"),
                importance_score: 0.5,
                decay_score: 0.5,
                pinned: false,
                tags: Vec::new(),
                embedding_id: None,
                token_count: None,
                source_events: Vec::new(),
                source_episode_ids: Vec::new(),
                corroboration_count: 0,
                relevance_score: 0.5,
                promoted_at: None,
                created_at: Utc::now(),
                updated_at: Utc::now(),
            },
            score: 1.0,
            rank,
        };
        let results = vec![
            make_result(Uuid::from_u128(1), 99),
            make_result(Uuid::from_u128(2), 99),
            make_result(Uuid::from_u128(3), 99),
        ];

        let page = paginate_results(&results, 2, 1);

        assert_eq!(page.len(), 2);
        assert_eq!(page[0].rank, 1);
        assert_eq!(page[1].rank, 2);
        assert_eq!(page[0].memory.id, Uuid::from_u128(2));
    }
}
