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

pub(crate) async fn hybrid_search_with_config(
    state: &AppState,
    req: &SearchRequest,
    limit: u32,
    workspace_config: &WorkspaceConfig,
) -> AppResult<Vec<MemoryResult>> {
    let offset = req.offset.unwrap_or(DEFAULT_OFFSET);
    let requested = limit.saturating_add(offset);
    let candidate_limit = requested.saturating_mul(2).max(limit.saturating_mul(2));
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
    let mut scores = HashMap::<Uuid, f32>::new();
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
    fn relevance_score_neutral_keeps_rrf_score() {
        let score = apply_relevance_score(0.8, 0.5);

        assert!((score - 0.8).abs() < 0.0001);
    }

    #[test]
    fn relevance_score_boosts_loved_and_dampens_hated() {
        let base = 0.8;
        let loved = apply_relevance_score(base, 1.0);
        let hated = apply_relevance_score(base, 0.0);

        assert!((loved - 0.84).abs() < 0.0001);
        assert!((hated - 0.76).abs() < 0.0001);
    }

    #[test]
    fn relevance_score_breaks_identical_content_tie() {
        let neutral = apply_relevance_score(0.8, 0.5);
        let loved = apply_relevance_score(0.8, 1.0);

        assert!(loved > neutral);
    }

    #[test]
    fn vector_empty_falls_back_to_keyword_only() {
        let id = Uuid::from_u128(10);
        let keyword = vec![memory_result(id, 0.75, 4)];

        let fallback = keyword_fallback_results(&keyword, 10);

        assert_eq!(fallback.len(), 1);
        assert_eq!(fallback[0].memory.id, id);
        assert_eq!(fallback[0].rank, 1);
        assert_eq!(fallback[0].score, 0.75);
    }

    #[test]
    fn paginate_results_skips_offset_and_resets_ranks() {
        let first = memory_result(Uuid::from_u128(1), 0.9, 10);
        let second = memory_result(Uuid::from_u128(2), 0.8, 11);
        let third = memory_result(Uuid::from_u128(3), 0.7, 12);

        let paged = paginate_results(&[first, second, third], 2, 1);

        assert_eq!(paged.len(), 2);
        assert_eq!(paged[0].memory.id, Uuid::from_u128(2));
        assert_eq!(paged[1].memory.id, Uuid::from_u128(3));
        assert_eq!(paged[0].rank, 1);
        assert_eq!(paged[1].rank, 2);
    }

    #[test]
    fn materialized_hybrid_results_reuse_existing_memory_payloads() {
        let id = Uuid::from_u128(7);
        let mut result = memory_result(id, 0.2, 9);
        result.memory.relevance_score = 1.0;

        let materialized = materialize_fused_results(
            vec![FusedRank {
                id,
                raw_score: 1.0,
                score: 0.8,
            }],
            vec![result],
            Vec::new(),
        );

        assert_eq!(materialized.len(), 1);
        assert_eq!(materialized[0].memory.id, id);
        assert!((materialized[0].score - 0.84).abs() < 0.0001);
        assert_eq!(materialized[0].rank, 0);
    }

    fn memory_result(id: Uuid, score: f32, rank: u32) -> MemoryResult {
        MemoryResult {
            memory: MemoryUnitDto {
                id,
                workspace_id: Uuid::from_u128(99),
                scope: json!({}),
                memory_type: "episodic".to_owned(),
                content: "memory".to_owned(),
                importance_score: 0.5,
                decay_score: 1.0,
                pinned: false,
                tags: Vec::new(),
                embedding_id: None,
                token_count: None,
                source_events: Vec::new(),
                source_episode_ids: Vec::new(),
                corroboration_count: 1,
                relevance_score: 0.5,
                promoted_at: None,
                scope_visibility: "private".to_owned(),
                created_at: Utc::now(),
                updated_at: Utc::now(),
            },
            score,
            rank,
        }
    }
}
