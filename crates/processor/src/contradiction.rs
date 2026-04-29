use anyhow::anyhow;
use chrono::Utc;
use common::{
    audit::spawn_audit_log,
    error::AppResult,
    models::{AuditAction, ContradictionMode, MemoryScope, MemoryUnit, WorkspaceConfig},
    AppError, AppState,
};
use qdrant_client::qdrant::{
    point_id::PointIdOptions, vector_output, vectors_output, Condition, Filter, GetPointsBuilder,
    PointId, RetrievedPoint, ScoredPoint, SearchPointsBuilder, VectorOutput,
};
use serde_json::json;
use sqlx::PgPool;
use uuid::Uuid;

use crate::embedder::COLLECTION_NAME;

const MEMORY_COLUMNS: &str = "id, workspace_id, scope, memory_type, content, entities, importance_score, importance_overridden, source_events, embedding_id, token_count, decay_score, pinned, tags, version, promoted_at, source_episode_ids, corroboration_count, deleted_at, last_accessed_at, created_at, updated_at";

/// Check `new_memory` against existing active memories in the same workspace and scope.
pub async fn check_contradictions(
    state: &AppState,
    new_memory: &MemoryUnit,
    config: &WorkspaceConfig,
) -> Result<(), AppError> {
    if new_memory.embedding_id.is_none() || config.contradiction_candidates == 0 {
        return Ok(());
    }

    let vector = match current_memory_vector(state, new_memory.id).await {
        Ok(Some(vector)) => vector,
        Ok(None) => return Ok(()),
        Err(error) => return Err(error),
    };
    let neighbours =
        search_neighbours(state, new_memory, vector, config.contradiction_candidates).await?;
    let candidate_ids = neighbours
        .iter()
        .filter_map(|candidate| {
            (candidate.memory_id != new_memory.id).then_some(candidate.memory_id)
        })
        .collect::<Vec<_>>();
    if candidate_ids.is_empty() {
        return Ok(());
    }

    let existing = fetch_active_memories_by_ids(&state.db, new_memory.workspace_id, &candidate_ids)
        .await?
        .into_iter()
        .filter(|memory| memory.scope == new_memory.scope)
        .map(|memory| (memory.id, memory))
        .collect::<std::collections::HashMap<_, _>>();
    let max_similarity = 1.0 - config.contradiction_threshold;

    for candidate in neighbours {
        if candidate.memory_id == new_memory.id || candidate.similarity >= max_similarity {
            continue;
        }
        let Some(existing_memory) = existing.get(&candidate.memory_id) else {
            continue;
        };

        let conflict_score = 1.0 - candidate.similarity;
        let (resolution, resolved_by, resolved_at) = match config.contradiction_mode {
            ContradictionMode::Quarantine => ("open", None, None),
            ContradictionMode::AutoResolve => {
                soft_delete_older_memory(&state.db, new_memory, existing_memory).await?;
                ("auto_resolved", Some("auto".to_owned()), Some(Utc::now()))
            }
        };

        if let Some(flag_id) = insert_contradiction_flag(
            &state.db,
            FlagWrite {
                workspace_id: new_memory.workspace_id,
                memory_id_a: existing_memory.id,
                memory_id_b: new_memory.id,
                similarity: candidate.similarity,
                conflict_score,
                resolution,
                resolved_by: resolved_by.as_deref(),
                resolved_at,
            },
        )
        .await?
        {
            spawn_audit_log(
                state.db.clone(),
                new_memory.workspace_id,
                "system".to_owned(),
                AuditAction::MemoryEdited,
                flag_id,
                "contradiction_flag",
                Some(json!({
                    "memory_id_a": existing_memory.id,
                    "memory_id_b": new_memory.id,
                    "similarity": candidate.similarity,
                    "conflict_score": conflict_score,
                    "resolution": resolution,
                })),
            );
        }
    }

    Ok(())
}

#[derive(Debug, Clone, Copy)]
struct VectorNeighbour {
    memory_id: Uuid,
    similarity: f32,
}

struct FlagWrite<'a> {
    workspace_id: Uuid,
    memory_id_a: Uuid,
    memory_id_b: Uuid,
    similarity: f32,
    conflict_score: f32,
    resolution: &'a str,
    resolved_by: Option<&'a str>,
    resolved_at: Option<chrono::DateTime<Utc>>,
}

async fn current_memory_vector(state: &AppState, memory_id: Uuid) -> AppResult<Option<Vec<f32>>> {
    let point_ids = vec![PointId::from(memory_id.to_string())];
    let response = state
        .qdrant
        .get_points(
            GetPointsBuilder::new(COLLECTION_NAME, point_ids)
                .with_payload(false)
                .with_vectors(true),
        )
        .await
        .map_err(|error| AppError::Internal(anyhow!(error)))?;

    Ok(response.result.first().and_then(dense_vector))
}

async fn search_neighbours(
    state: &AppState,
    memory: &MemoryUnit,
    vector: Vec<f32>,
    candidates: usize,
) -> AppResult<Vec<VectorNeighbour>> {
    let limit = u64::try_from(candidates.saturating_add(1)).unwrap_or(u64::MAX);
    let response = state
        .qdrant
        .search_points(
            SearchPointsBuilder::new(COLLECTION_NAME, vector, limit)
                .filter(scope_filter(memory.workspace_id, &memory.scope)),
        )
        .await
        .map_err(|error| AppError::Internal(anyhow!(error)))?;

    Ok(response
        .result
        .iter()
        .filter_map(|point| {
            scored_point_uuid(point).map(|memory_id| VectorNeighbour {
                memory_id,
                similarity: point.score.clamp(-1.0, 1.0),
            })
        })
        .collect())
}

fn scope_filter(workspace_id: Uuid, scope: &MemoryScope) -> Filter {
    let mut conditions = vec![Condition::matches("workspace_id", workspace_id.to_string())];
    if let Some(agent_id) = &scope.agent_id {
        conditions.push(Condition::matches("agent_id", agent_id.clone()));
    }
    if let Some(user_id) = &scope.user_id {
        conditions.push(Condition::matches("user_id", user_id.clone()));
    }
    if let Some(repo) = &scope.repo {
        conditions.push(Condition::matches("repo", repo.clone()));
    }

    Filter::must(conditions)
}

async fn fetch_active_memories_by_ids(
    db: &PgPool,
    workspace_id: Uuid,
    ids: &[Uuid],
) -> AppResult<Vec<MemoryUnit>> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }

    let sql = format!(
        "SELECT {MEMORY_COLUMNS} FROM memory_units WHERE workspace_id = $1 AND id = ANY($2) AND deleted_at IS NULL"
    );
    sqlx::query_as::<_, MemoryUnit>(&sql)
        .bind(workspace_id)
        .bind(ids.to_vec())
        .fetch_all(db)
        .await
        .map_err(AppError::Database)
}

async fn soft_delete_older_memory(
    db: &PgPool,
    new_memory: &MemoryUnit,
    existing_memory: &MemoryUnit,
) -> AppResult<()> {
    let older_id = if existing_memory.created_at <= new_memory.created_at {
        existing_memory.id
    } else {
        new_memory.id
    };

    sqlx::query(
        r#"
        UPDATE memory_units
        SET deleted_at = now(), embedding_id = NULL, version = version + 1
        WHERE workspace_id = $1 AND id = $2 AND deleted_at IS NULL
        "#,
    )
    .bind(new_memory.workspace_id)
    .bind(older_id)
    .execute(db)
    .await
    .map(|_| ())
    .map_err(AppError::Database)
}

async fn insert_contradiction_flag(db: &PgPool, flag: FlagWrite<'_>) -> AppResult<Option<Uuid>> {
    sqlx::query_scalar::<_, Uuid>(
        r#"
        INSERT INTO contradiction_flags (
            workspace_id, memory_id_a, memory_id_b, similarity, conflict_score,
            resolution, resolved_by, resolved_at
        )
        SELECT $1, $2, $3, $4, $5, $6::contradiction_resolution, $7, $8
        WHERE NOT EXISTS (
            SELECT 1
            FROM contradiction_flags
            WHERE workspace_id = $1
              AND (
                  (memory_id_a = $2 AND memory_id_b = $3)
                  OR (memory_id_a = $3 AND memory_id_b = $2)
              )
        )
        RETURNING id
        "#,
    )
    .bind(flag.workspace_id)
    .bind(flag.memory_id_a)
    .bind(flag.memory_id_b)
    .bind(flag.similarity)
    .bind(flag.conflict_score)
    .bind(flag.resolution)
    .bind(flag.resolved_by)
    .bind(flag.resolved_at)
    .fetch_optional(db)
    .await
    .map_err(AppError::Database)
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

fn scored_point_uuid(point: &ScoredPoint) -> Option<Uuid> {
    match point.id.as_ref()?.point_id_options.as_ref()? {
        PointIdOptions::Uuid(value) => Uuid::parse_str(value).ok(),
        PointIdOptions::Num(_) => None,
    }
}

pub async fn fetch_workspace_config(db: &PgPool, workspace_id: Uuid) -> AppResult<WorkspaceConfig> {
    let value = sqlx::query_scalar::<_, serde_json::Value>(
        "SELECT config FROM workspaces WHERE id = $1 AND deleted_at IS NULL",
    )
    .bind(workspace_id)
    .fetch_optional(db)
    .await
    .map_err(AppError::Database)?
    .ok_or_else(|| AppError::NotFound {
        resource: format!("workspace:{workspace_id}"),
    })?;

    serde_json::from_value::<WorkspaceConfig>(value)
        .map_err(|error| AppError::Internal(anyhow!(error)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scope_filter_includes_present_scope_fields() {
        let workspace_id = Uuid::now_v7();
        let scope = MemoryScope {
            workspace_id,
            agent_id: Some("agent".to_owned()),
            user_id: None,
            repo: Some("Quazmoz/memoryops".to_owned()),
        };

        let filter = scope_filter(workspace_id, &scope);
        let debug = format!("{filter:?}");

        assert!(debug.contains("workspace_id"));
        assert!(debug.contains("agent_id"));
        assert!(debug.contains("repo"));
    }

    #[test]
    fn default_config_uses_quarantine_threshold() {
        let config = WorkspaceConfig::default();

        assert_eq!(config.contradiction_mode, ContradictionMode::Quarantine);
        assert_eq!(config.contradiction_threshold, 0.35);
        assert_eq!(config.contradiction_candidates, 20);
    }
}
