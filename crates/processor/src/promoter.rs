use std::{collections::HashMap, time::Duration};

use anyhow::{anyhow, Context};
use chrono::Utc;
use common::{
    models::{MemoryType, MemoryUnit, ScopeVisibility},
    providers::{EmbeddingProvider, LlmProvider},
};
use qdrant_client::{
    qdrant::{
        point_id::PointIdOptions, vector_output, vectors_output, DeletePointsBuilder,
        GetPointsBuilder, PointId, PointStruct, RetrievedPoint, UpsertPointsBuilder, VectorOutput,
    },
    Qdrant as QdrantClient,
};
use serde::Serialize;
use serde_json::json;
use sqlx::PgPool;
use uuid::Uuid;

use crate::embedder::{QdrantPayload, COLLECTION_NAME};

const MEMORY_COLUMNS: &str = "id, workspace_id, scope, memory_type, scope_visibility, content, entities, importance_score, importance_overridden, source_events, embedding_id, token_count, decay_score, pinned, tags, version, promoted_at, source_episode_ids, corroboration_count, deleted_at, last_accessed_at, created_at, updated_at";
const PROMOTION_SUMMARY_MAX_TOKENS: usize = 256;
const EMBED_MAX_ATTEMPTS: usize = 3;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PromoterConfig {
    pub promotion_threshold: f32,
    pub dedup_cosine_threshold: f32,
    pub cluster_min_size: usize,
    pub batch_size: i64,
}

impl Default for PromoterConfig {
    fn default() -> Self {
        Self {
            promotion_threshold: 0.72,
            dedup_cosine_threshold: 0.92,
            cluster_min_size: 3,
            batch_size: 200,
        }
    }
}

impl PromoterConfig {
    fn normalized(self) -> Self {
        Self {
            cluster_min_size: self.cluster_min_size.max(1),
            batch_size: self.batch_size.max(1),
            ..self
        }
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct PromotionReport {
    pub workspace_id: Uuid,
    pub clusters_found: usize,
    pub units_promoted: usize,
    pub units_skipped: usize,
}

pub async fn run_promotion_pass(
    pool: &PgPool,
    qdrant: &QdrantClient,
    llm: &dyn LlmProvider,
    embedder: &dyn EmbeddingProvider,
    workspace_id: Uuid,
    config: PromoterConfig,
) -> anyhow::Result<PromotionReport> {
    let config = config.normalized();
    let candidates = fetch_candidates(pool, workspace_id, config).await?;
    let mut report = PromotionReport {
        workspace_id,
        clusters_found: 0,
        units_promoted: 0,
        units_skipped: 0,
    };

    if candidates.is_empty() {
        return Ok(report);
    }

    let vector_candidates = match fetch_candidate_vectors(qdrant, &candidates).await {
        Ok(vector_candidates) => vector_candidates,
        Err(error) => {
            tracing::warn!(error = ?error, workspace_id = %workspace_id, "Qdrant unavailable during promotion pass; skipping vector clustering");
            return Ok(report);
        }
    };
    report.units_skipped = candidates.len().saturating_sub(vector_candidates.len());

    let plan = cluster_candidate_vectors(
        &vector_candidates,
        config.dedup_cosine_threshold,
        config.cluster_min_size,
    );
    report.clusters_found = plan.clusters.len();
    report.units_skipped = report.units_skipped.saturating_add(plan.units_skipped);

    let units_by_id = candidates
        .into_iter()
        .map(|unit| (unit.id, unit))
        .collect::<HashMap<_, _>>();

    for cluster_ids in plan.clusters {
        let cluster_units = cluster_ids
            .iter()
            .filter_map(|id| units_by_id.get(id).cloned())
            .collect::<Vec<_>>();
        if cluster_units.len() < config.cluster_min_size {
            report.units_skipped = report.units_skipped.saturating_add(cluster_units.len());
            continue;
        }

        let cluster_text = cluster_units
            .iter()
            .map(|unit| unit.content.as_str())
            .collect::<Vec<_>>()
            .join("\n---\n");
        let semantic_summary = match llm
            .summarize(&cluster_text, PROMOTION_SUMMARY_MAX_TOKENS)
            .await
        {
            Ok(summary) if summary.trim().is_empty() => cluster_text,
            Ok(summary) => summary,
            Err(error) => {
                tracing::warn!(error = ?error, workspace_id = %workspace_id, "promotion LLM summarization failed; skipping cluster");
                continue;
            }
        };

        let semantic_id = Uuid::now_v7();
        let importance_score = average_importance(&cluster_units);
        let decay_score = max_decay(&cluster_units);
        let embedding_id = match embed_semantic_with_retries(
            qdrant,
            embedder,
            workspace_id,
            semantic_id,
            &semantic_summary,
            importance_score,
            decay_score,
        )
        .await
        {
            Ok(embedding_id) => embedding_id,
            Err(error) => {
                tracing::error!(error = ?error, workspace_id = %workspace_id, semantic_id = %semantic_id, "promotion embedding failed after retries; skipping cluster");
                continue;
            }
        };

        let source_ids = cluster_units.iter().map(|unit| unit.id).collect::<Vec<_>>();
        insert_semantic_unit_and_delete_sources(
            pool,
            SemanticWrite {
                workspace_id,
                semantic_id,
                semantic_summary: &semantic_summary,
                importance_score,
                decay_score,
                embedding_id: &embedding_id,
                source_ids: &source_ids,
            },
        )
        .await?;

        delete_source_points(qdrant, &source_ids).await;
        report.units_promoted = report.units_promoted.saturating_add(1);
    }

    Ok(report)
}

async fn fetch_candidates(
    pool: &PgPool,
    workspace_id: Uuid,
    config: PromoterConfig,
) -> anyhow::Result<Vec<MemoryUnit>> {
    let sql = format!(
        r#"
        SELECT {MEMORY_COLUMNS}
        FROM memory_units
        WHERE workspace_id = $1
          AND embedding_id IS NOT NULL
          AND deleted_at IS NULL
          AND decay_score >= $2
          AND memory_type = 'episodic'
        ORDER BY decay_score DESC
        LIMIT $3
        "#
    );

    sqlx::query_as::<_, MemoryUnit>(&sql)
        .bind(workspace_id)
        .bind(config.promotion_threshold)
        .bind(config.batch_size)
        .fetch_all(pool)
        .await
        .context("failed to fetch promotion candidates")
}

async fn fetch_candidate_vectors(
    qdrant: &QdrantClient,
    candidates: &[MemoryUnit],
) -> anyhow::Result<Vec<VectorCandidate>> {
    let point_ids = candidates
        .iter()
        .map(|candidate| candidate.id.to_string().into())
        .collect::<Vec<PointId>>();
    let response = qdrant
        .get_points(
            GetPointsBuilder::new(COLLECTION_NAME, point_ids)
                .with_payload(false)
                .with_vectors(true),
        )
        .await
        .context("failed to fetch candidate vectors from Qdrant")?;

    let vector_candidates = response
        .result
        .iter()
        .filter_map(|point| Some((retrieved_point_uuid(point)?, dense_vector(point)?)))
        .map(|(memory_id, vector)| VectorCandidate { memory_id, vector })
        .collect();

    Ok(vector_candidates)
}

async fn embed_semantic_with_retries(
    qdrant: &QdrantClient,
    embedder: &dyn EmbeddingProvider,
    workspace_id: Uuid,
    semantic_id: Uuid,
    semantic_summary: &str,
    importance_score: f32,
    decay_score: f32,
) -> anyhow::Result<String> {
    let mut last_error = None;

    for attempt in 1..=EMBED_MAX_ATTEMPTS {
        match embed_and_store_semantic(
            qdrant,
            embedder,
            workspace_id,
            semantic_id,
            semantic_summary,
            importance_score,
            decay_score,
        )
        .await
        {
            Ok(embedding_id) => return Ok(embedding_id),
            Err(error) => {
                last_error = Some(error);
                if attempt < EMBED_MAX_ATTEMPTS {
                    tokio::time::sleep(Duration::from_millis(100 * attempt as u64)).await;
                }
            }
        }
    }

    Err(last_error.unwrap_or_else(|| anyhow!("embedding failed")))
}

async fn embed_and_store_semantic(
    qdrant: &QdrantClient,
    embedder: &dyn EmbeddingProvider,
    workspace_id: Uuid,
    semantic_id: Uuid,
    semantic_summary: &str,
    importance_score: f32,
    decay_score: f32,
) -> anyhow::Result<String> {
    let vector = embedder
        .embed(semantic_summary)
        .await
        .map_err(|error| anyhow!(error))?;
    let embedding_id = semantic_id.to_string();
    let payload = QdrantPayload {
        workspace_id,
        memory_type: MemoryType::Semantic,
        scope_visibility: ScopeVisibility::Private,
        importance_score,
        decay_score,
        created_at: Utc::now(),
        agent_id: None,
        user_id: None,
        repo: None,
        tags: Vec::new(),
    };
    let point = PointStruct::new(embedding_id.clone(), vector, payload.into_qdrant_payload());
    qdrant
        .upsert_points(UpsertPointsBuilder::new(COLLECTION_NAME, vec![point]).wait(true))
        .await
        .context("failed to write semantic vector to Qdrant")?;

    Ok(embedding_id)
}

struct SemanticWrite<'a> {
    workspace_id: Uuid,
    semantic_id: Uuid,
    semantic_summary: &'a str,
    importance_score: f32,
    decay_score: f32,
    embedding_id: &'a str,
    source_ids: &'a [Uuid],
}

async fn insert_semantic_unit_and_delete_sources(
    pool: &PgPool,
    semantic: SemanticWrite<'_>,
) -> anyhow::Result<()> {
    let mut transaction = pool
        .begin()
        .await
        .context("failed to begin promotion transaction")?;
    let promoted_at = Utc::now();

    sqlx::query(
        r#"
        INSERT INTO memory_units (
            id,
            workspace_id,
            scope,
            memory_type,
            content,
            entities,
            importance_score,
            decay_score,
            embedding_id,
            promoted_at,
            source_episode_ids,
            corroboration_count,
            tags
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)
        "#,
    )
    .bind(semantic.semantic_id)
    .bind(semantic.workspace_id)
    .bind(json!({
        "workspace_id": semantic.workspace_id,
        "agent_id": null,
        "user_id": null,
        "repo": null
    }))
    .bind(MemoryType::Semantic)
    .bind(semantic.semantic_summary)
    .bind(json!([]))
    .bind(semantic.importance_score)
    .bind(semantic.decay_score)
    .bind(semantic.embedding_id)
    .bind(promoted_at)
    .bind(semantic.source_ids.to_vec())
    .bind(i32::try_from(semantic.source_ids.len()).unwrap_or(i32::MAX))
    .bind(Vec::<String>::new())
    .execute(&mut *transaction)
    .await
    .context("failed to insert semantic memory unit")?;

    sqlx::query(
        r#"
        UPDATE memory_units
        SET deleted_at = now(),
            updated_at = now(),
            embedding_id = NULL,
            version = version + 1
        WHERE workspace_id = $1
          AND id = ANY($2)
          AND deleted_at IS NULL
        "#,
    )
    .bind(semantic.workspace_id)
    .bind(semantic.source_ids.to_vec())
    .execute(&mut *transaction)
    .await
    .context("failed to soft-delete promoted source episodes")?;

    transaction
        .commit()
        .await
        .context("failed to commit promotion transaction")
}

async fn delete_source_points(qdrant: &QdrantClient, source_ids: &[Uuid]) {
    for source_id in source_ids {
        if let Err(error) = qdrant
            .delete_points(
                DeletePointsBuilder::new(COLLECTION_NAME)
                    .points([source_id.to_string()])
                    .wait(true),
            )
            .await
        {
            tracing::warn!(error = ?error, memory_id = %source_id, "failed to delete source episode point after promotion");
        }
    }
}

fn average_importance(units: &[MemoryUnit]) -> f32 {
    if units.is_empty() {
        return 0.0;
    }

    units.iter().map(|unit| unit.importance_score).sum::<f32>() / units.len() as f32
}

fn max_decay(units: &[MemoryUnit]) -> f32 {
    units
        .iter()
        .map(|unit| unit.decay_score)
        .fold(0.0_f32, f32::max)
}

fn dense_vector(point: &RetrievedPoint) -> Option<Vec<f32>> {
    let vectors = point.vectors.as_ref()?.vectors_options.as_ref()?;
    match vectors {
        vectors_output::VectorsOptions::Vector(vector) => dense_vector_output(vector),
        vectors_output::VectorsOptions::Vectors(named) => {
            named.vectors.values().find_map(dense_vector_output)
        }
    }
}

fn dense_vector_output(vector: &VectorOutput) -> Option<Vec<f32>> {
    match vector.vector.as_ref()? {
        vector_output::Vector::Dense(dense) => Some(dense.data.clone()),
        vector_output::Vector::Sparse(_) | vector_output::Vector::MultiDense(_) => None,
    }
}

fn retrieved_point_uuid(point: &RetrievedPoint) -> Option<Uuid> {
    match point.id.as_ref()?.point_id_options.as_ref()? {
        PointIdOptions::Uuid(value) => Uuid::parse_str(value).ok(),
        PointIdOptions::Num(_) => None,
    }
}

#[derive(Debug, Clone, PartialEq)]
struct VectorCandidate {
    memory_id: Uuid,
    vector: Vec<f32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ClusterPlan {
    clusters: Vec<Vec<Uuid>>,
    units_skipped: usize,
}

fn cluster_candidate_vectors(
    candidates: &[VectorCandidate],
    threshold: f32,
    min_size: usize,
) -> ClusterPlan {
    let min_size = min_size.max(1);
    let mut assigned = vec![false; candidates.len()];
    let mut clusters = Vec::new();
    let mut units_skipped = 0_usize;

    for seed_index in 0..candidates.len() {
        if assigned[seed_index] {
            continue;
        }

        let cluster_indices = candidates
            .iter()
            .enumerate()
            .filter_map(|(candidate_index, candidate)| {
                if assigned[candidate_index] {
                    return None;
                }
                (cosine_sim(&candidates[seed_index].vector, &candidate.vector) >= threshold)
                    .then_some(candidate_index)
            })
            .collect::<Vec<_>>();

        for index in &cluster_indices {
            assigned[*index] = true;
        }

        if cluster_indices.len() >= min_size {
            clusters.push(
                cluster_indices
                    .iter()
                    .map(|index| candidates[*index].memory_id)
                    .collect(),
            );
        } else {
            units_skipped = units_skipped.saturating_add(cluster_indices.len());
        }
    }

    ClusterPlan {
        clusters,
        units_skipped,
    }
}

#[cfg(test)]
fn report_for_cluster_plan(workspace_id: Uuid, plan: &ClusterPlan) -> PromotionReport {
    PromotionReport {
        workspace_id,
        clusters_found: plan.clusters.len(),
        units_promoted: plan.clusters.len(),
        units_skipped: plan.units_skipped,
    }
}

fn cosine_sim(a: &[f32], b: &[f32]) -> f32 {
    a.iter()
        .zip(b.iter())
        .map(|(left, right)| left * right)
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cosine_sim_of_identical_vectors_is_one() {
        let vector = [1.0, 0.0, 0.0];

        assert!((cosine_sim(&vector, &vector) - 1.0).abs() < 0.0001);
    }

    #[test]
    fn cosine_sim_of_orthogonal_vectors_is_zero() {
        let left = [1.0, 0.0, 0.0];
        let right = [0.0, 1.0, 0.0];

        assert!(cosine_sim(&left, &right).abs() < 0.0001);
    }

    #[test]
    fn cluster_below_min_size_is_skipped() {
        let candidates = vector_candidates(&[[1.0, 0.0, 0.0], [1.0, 0.0, 0.0]]);

        let plan = cluster_candidate_vectors(&candidates, 0.92, 3);

        assert!(plan.clusters.is_empty());
        assert_eq!(plan.units_skipped, 2);
    }

    #[test]
    fn cluster_at_min_size_is_included() {
        let candidates = vector_candidates(&[[1.0, 0.0, 0.0], [1.0, 0.0, 0.0], [1.0, 0.0, 0.0]]);

        let plan = cluster_candidate_vectors(&candidates, 0.92, 3);

        assert_eq!(plan.clusters.len(), 1);
        assert_eq!(plan.clusters[0].len(), 3);
        assert_eq!(plan.units_skipped, 0);
    }

    #[test]
    fn promotion_report_counts_are_accurate() {
        let candidates = vector_candidates(&[
            [1.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, 0.0, 1.0],
            [0.0, 0.0, 1.0],
        ]);
        let plan = cluster_candidate_vectors(&candidates, 0.92, 3);
        let report = report_for_cluster_plan(Uuid::now_v7(), &plan);

        assert_eq!(report.clusters_found, 2);
        assert_eq!(report.units_promoted, 2);
        assert_eq!(report.units_skipped, 2);
    }

    fn vector_candidates(vectors: &[[f32; 3]]) -> Vec<VectorCandidate> {
        vectors
            .iter()
            .map(|vector| VectorCandidate {
                memory_id: Uuid::now_v7(),
                vector: vector.to_vec(),
            })
            .collect()
    }
}
