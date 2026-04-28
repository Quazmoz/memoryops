use std::collections::HashMap;

use common::{error::AppResult, AppState};
use uuid::Uuid;

use crate::{
    dto::{rank_from_index, MemoryResult, MemoryUnitDto, SearchRequest},
    store,
};

use super::{keyword::keyword_search, vector::vector_search_results};

pub const RRF_K: f32 = 60.0;

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
    let candidate_limit = limit.saturating_mul(2);
    let (vector_results, keyword_results) = tokio::join!(
        vector_search_results(state, req, candidate_limit),
        keyword_search(state, req, candidate_limit)
    );

    let vector_results = vector_results?;
    let keyword_results = keyword_results?;

    if vector_results.is_empty() {
        return Ok(keyword_fallback_results(&keyword_results, limit as usize));
    }

    let vector_ids = vector_results
        .iter()
        .map(|result| result.memory.id)
        .collect::<Vec<_>>();
    let keyword_ids = keyword_results
        .iter()
        .map(|result| result.memory.id)
        .collect::<Vec<_>>();
    let fused = fuse_ranked_ids(&vector_ids, &keyword_ids, limit as usize);
    let ids = fused.iter().map(|rank| rank.id).collect::<Vec<_>>();
    let units = store::get_memory_units_by_ids(&state.db, &ids, req.workspace_id).await?;
    let mut units_by_id = units
        .into_iter()
        .map(|unit| (unit.id, unit))
        .collect::<HashMap<_, _>>();

    let mut results = Vec::with_capacity(fused.len());
    for fused_rank in fused {
        if let Some(unit) = units_by_id.remove(&fused_rank.id) {
            let rank = rank_from_index(results.len());
            results.push(MemoryResult {
                memory: MemoryUnitDto::from(unit),
                score: fused_rank.score,
                rank,
            });
        }
    }

    Ok(results)
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

pub fn keyword_fallback_results(results: &[MemoryResult], limit: usize) -> Vec<MemoryResult> {
    results
        .iter()
        .take(limit)
        .cloned()
        .enumerate()
        .map(|(index, mut result)| {
            result.rank = rank_from_index(index);
            result
        })
        .collect()
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
    fn vector_empty_falls_back_to_keyword_only() {
        let id = Uuid::from_u128(10);
        let keyword = vec![memory_result(id, 0.75, 4)];

        let fallback = keyword_fallback_results(&keyword, 10);

        assert_eq!(fallback.len(), 1);
        assert_eq!(fallback[0].memory.id, id);
        assert_eq!(fallback[0].rank, 1);
        assert_eq!(fallback[0].score, 0.75);
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
                created_at: Utc::now(),
                updated_at: Utc::now(),
            },
            score,
            rank,
        }
    }
}
