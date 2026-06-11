use std::collections::{HashMap, HashSet};

use anyhow::anyhow;
use chrono::Utc;
use common::{
    audit::spawn_audit_log,
    error::AppResult,
    models::{AuditAction, ContradictionMode, MemoryScope, MemoryType, MemoryUnit, WorkspaceConfig},
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

const MEMORY_COLUMNS: &str = "id, workspace_id, scope, memory_type, scope_visibility, content, entities, importance_score, importance_overridden, source_events, embedding_id, token_count, decay_score, relevance_score, pinned, tags, version, promoted_at, source_episode_ids, corroboration_count, deleted_at, last_accessed_at, created_at, updated_at";
const MIN_RELATED_SIMILARITY: f32 = 0.50;
const MAX_RELATED_SIMILARITY: f32 = 0.98;
const DEFAULT_CONFIDENCE_FLOOR: f32 = 0.35;
const AUTO_RESOLVE_MIN_CONFIDENCE: f32 = 0.80;
const PHRASE_CONFLICT_CONFIDENCE: f32 = 0.90;
const NEGATION_CONFLICT_CONFIDENCE: f32 = 0.80;
const NUMERIC_CONFLICT_CONFIDENCE: f32 = 0.75;

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
        .collect::<HashMap<_, _>>();
    let min_similarity = related_similarity_floor(config.contradiction_threshold);
    let confidence_floor = contradiction_confidence_floor(config.contradiction_threshold);

    for candidate in neighbours {
        if candidate.memory_id == new_memory.id || candidate.similarity < min_similarity {
            continue;
        }
        let Some(existing_memory) = existing.get(&candidate.memory_id) else {
            continue;
        };
        let Some(conflict_score) = contradiction_confidence(
            &existing_memory.content,
            &new_memory.content,
            candidate.similarity,
        ) else {
            continue;
        };
        if conflict_score < confidence_floor {
            continue;
        }

        let (resolution, resolved_by, resolved_at) = match config.contradiction_mode {
            ContradictionMode::Quarantine => ("open", None, None),
            ContradictionMode::AutoResolve if conflict_score >= AUTO_RESOLVE_MIN_CONFIDENCE => {
                let discarded_id = choose_auto_resolve_discarded_memory(new_memory, existing_memory);
                soft_delete_memory_by_id(&state.db, new_memory.workspace_id, discarded_id).await?;
                ("auto_resolved", Some("auto".to_owned()), Some(Utc::now()))
            }
            ContradictionMode::AutoResolve => ("open", None, None),
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

fn related_similarity_floor(contradiction_threshold: f32) -> f32 {
    (1.0 - contradiction_threshold).clamp(MIN_RELATED_SIMILARITY, MAX_RELATED_SIMILARITY)
}

fn contradiction_confidence_floor(contradiction_threshold: f32) -> f32 {
    contradiction_threshold.clamp(0.20, 0.95).max(DEFAULT_CONFIDENCE_FLOOR)
}

fn contradiction_confidence(left: &str, right: &str, similarity: f32) -> Option<f32> {
    let left_words = normalized_word_set(left);
    let right_words = normalized_word_set(right);
    if meaningful_overlap_count(&left_words, &right_words) < 2 {
        return None;
    }

    let mut confidence = 0.0_f32;
    if has_direct_phrase_conflict(&left_words, &right_words) {
        confidence = confidence.max(PHRASE_CONFLICT_CONFIDENCE);
    }
    if has_negation_conflict(left, right, &left_words, &right_words) {
        confidence = confidence.max(NEGATION_CONFLICT_CONFIDENCE);
    }
    if has_numeric_conflict(left, right, &left_words, &right_words) {
        confidence = confidence.max(NUMERIC_CONFLICT_CONFIDENCE);
    }

    if confidence <= 0.0 {
        return None;
    }

    let relatedness_weight = 0.5 + (similarity.clamp(0.0, 1.0) * 0.5);
    Some((confidence * relatedness_weight).clamp(0.0, 1.0))
}

fn has_direct_phrase_conflict(left_words: &HashSet<String>, right_words: &HashSet<String>) -> bool {
    const CONFLICT_PAIRS: &[(&str, &str)] = &[
        ("enabled", "disabled"),
        ("enable", "disable"),
        ("true", "false"),
        ("yes", "no"),
        ("allowed", "denied"),
        ("allow", "deny"),
        ("passed", "failed"),
        ("success", "failure"),
        ("succeed", "fail"),
        ("working", "broken"),
        ("work", "fail"),
        ("open", "closed"),
        ("blocked", "unblocked"),
        ("available", "unavailable"),
        ("supported", "unsupported"),
        ("support", "unsupported"),
        ("active", "inactive"),
        ("public", "private"),
    ];

    CONFLICT_PAIRS.iter().any(|(left, right)| {
        (left_words.contains(*left) && right_words.contains(*right))
            || (left_words.contains(*right) && right_words.contains(*left))
    })
}

fn has_negation_conflict(
    left: &str,
    right: &str,
    left_words: &HashSet<String>,
    right_words: &HashSet<String>,
) -> bool {
    let left_negated = negated_terms(left);
    let right_negated = negated_terms(right);
    (!left_negated.is_empty()
        && left_negated
            .iter()
            .any(|term| right_words.contains(term) && !right_negated.contains(term)))
        || (!right_negated.is_empty()
            && right_negated
                .iter()
                .any(|term| left_words.contains(term) && !left_negated.contains(term)))
}

fn has_numeric_conflict(
    left: &str,
    right: &str,
    left_words: &HashSet<String>,
    right_words: &HashSet<String>,
) -> bool {
    if meaningful_overlap_count(left_words, right_words) < 3 {
        return false;
    }

    let left_numbers = extract_numbers(left);
    let right_numbers = extract_numbers(right);
    !left_numbers.is_empty() && !right_numbers.is_empty() && left_numbers != right_numbers
}

fn normalized_word_set(text: &str) -> HashSet<String> {
    text.split(|ch: char| !ch.is_ascii_alphanumeric() && ch != '\'')
        .filter_map(normalize_word)
        .filter(|word| !is_stop_word(word))
        .collect()
}

fn negated_terms(text: &str) -> HashSet<String> {
    let words = text
        .split(|ch: char| !ch.is_ascii_alphanumeric() && ch != '\'')
        .filter_map(normalize_word)
        .collect::<Vec<_>>();
    let mut negated = HashSet::new();

    for (index, word) in words.iter().enumerate() {
        if is_negation(word) {
            for candidate in words.iter().skip(index + 1).take(2) {
                if !is_stop_word(candidate) && !is_negation(candidate) {
                    negated.insert(candidate.clone());
                }
            }
        }
    }

    negated
}

fn extract_numbers(text: &str) -> Vec<String> {
    let mut numbers = Vec::new();
    let mut current = String::new();

    for ch in text.chars() {
        if ch.is_ascii_digit() || (ch == '.' && current.chars().any(|existing| existing.is_ascii_digit())) {
            current.push(ch);
        } else if !current.is_empty() {
            push_number_token(&mut numbers, &mut current);
        }
    }
    if !current.is_empty() {
        push_number_token(&mut numbers, &mut current);
    }

    numbers.sort();
    numbers.dedup();
    numbers
}

fn push_number_token(numbers: &mut Vec<String>, current: &mut String) {
    let token = current.trim_matches('.');
    if !token.is_empty() && token.chars().any(|ch| ch.is_ascii_digit()) {
        numbers.push(token.to_owned());
    }
    current.clear();
}

fn meaningful_overlap_count(left_words: &HashSet<String>, right_words: &HashSet<String>) -> usize {
    left_words.intersection(right_words).count()
}

fn normalize_word(raw: &str) -> Option<String> {
    let word = raw.trim_matches('\'').to_ascii_lowercase();
    if word.is_empty() {
        return None;
    }

    Some(stem_word(&word))
}

fn stem_word(word: &str) -> String {
    if word.len() > 5 && word.ends_with("ing") {
        return word[..word.len() - 3].to_owned();
    }
    if word.len() > 4 && word.ends_with("ed") {
        return word[..word.len() - 2].to_owned();
    }
    if word.len() > 3 && word.ends_with('s') {
        return word[..word.len() - 1].to_owned();
    }
    word.to_owned()
}

fn is_negation(word: &str) -> bool {
    matches!(
        word,
        "not"
            | "no"
            | "never"
            | "without"
            | "cannot"
            | "can't"
            | "won't"
            | "doesn't"
            | "isn't"
            | "aren't"
            | "shouldn't"
            | "mustn't"
    )
}

fn is_stop_word(word: &str) -> bool {
    matches!(
        word,
        "a" | "an"
            | "and"
            | "are"
            | "as"
            | "at"
            | "be"
            | "by"
            | "for"
            | "from"
            | "has"
            | "have"
            | "in"
            | "into"
            | "is"
            | "it"
            | "of"
            | "on"
            | "or"
            | "that"
            | "the"
            | "this"
            | "to"
            | "with"
    )
}

fn choose_auto_resolve_discarded_memory(new_memory: &MemoryUnit, existing_memory: &MemoryUnit) -> Uuid {
    let new_score = memory_trust_score(new_memory);
    let existing_score = memory_trust_score(existing_memory);

    if (new_score - existing_score).abs() > f32::EPSILON {
        if new_score > existing_score {
            existing_memory.id
        } else {
            new_memory.id
        }
    } else if existing_memory.created_at <= new_memory.created_at {
        existing_memory.id
    } else {
        new_memory.id
    }
}

fn memory_trust_score(memory: &MemoryUnit) -> f32 {
    let mut score = 0.0;
    if memory.pinned {
        score += 100.0;
    }
    if matches!(memory.memory_type, MemoryType::Semantic) {
        score += 25.0;
    }
    score += memory.corroboration_count.clamp(0, 10) as f32 * 2.0;
    score += memory.importance_score.clamp(0.0, 1.0) * 10.0;
    score += memory.relevance_score.clamp(0.0, 1.0) as f32 * 5.0;
    score
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

async fn soft_delete_memory_by_id(
    db: &PgPool,
    workspace_id: Uuid,
    memory_id: Uuid,
) -> AppResult<()> {
    sqlx::query(
        r#"
        UPDATE memory_units
        SET deleted_at = now(), embedding_id = NULL, version = version + 1
        WHERE workspace_id = $1 AND id = $2 AND deleted_at IS NULL
        "#,
    )
    .bind(workspace_id)
    .bind(memory_id)
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
    common::services::WorkspaceConfigService::new(db.clone())
        .load(workspace_id)
        .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scope_filter_includes_present_scope_fields() {
        let workspace_id = Uuid::now_v7();
        let scope = MemoryScope {
            workspace_id,
            source: None,
            actor: None,
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

    #[test]
    fn related_similarity_floor_treats_threshold_as_max_distance() {
        assert_eq!(related_similarity_floor(0.35), 0.65);
        assert_eq!(related_similarity_floor(0.0), MAX_RELATED_SIMILARITY);
        assert_eq!(related_similarity_floor(0.9), MIN_RELATED_SIMILARITY);
    }

    #[test]
    fn contradiction_confidence_ignores_near_duplicates_without_conflict_signal() {
        let confidence = contradiction_confidence(
            "MemoryOps supports workspace-scoped semantic memory for agents.",
            "MemoryOps supports workspace-scoped semantic memory for agents.",
            0.96,
        );

        assert!(confidence.is_none());
    }

    #[test]
    fn contradiction_confidence_detects_direct_status_conflict() {
        let confidence = contradiction_confidence(
            "The GitHub integration is enabled for the memoryops repo.",
            "The GitHub integration is disabled for the memoryops repo.",
            0.88,
        )
        .expect("status conflict should be detected");

        assert!(confidence >= DEFAULT_CONFIDENCE_FLOOR);
    }

    #[test]
    fn contradiction_confidence_detects_negation_conflict() {
        let confidence = contradiction_confidence(
            "The API key rotation job supports workspace scoped keys.",
            "The API key rotation job does not support workspace scoped keys.",
            0.84,
        )
        .expect("negation conflict should be detected");

        assert!(confidence >= DEFAULT_CONFIDENCE_FLOOR);
    }

    #[test]
    fn contradiction_confidence_detects_numeric_conflict_with_shared_context() {
        let confidence = contradiction_confidence(
            "The retention window for workspace audit logs is 30 days.",
            "The retention window for workspace audit logs is 90 days.",
            0.91,
        )
        .expect("numeric conflict should be detected");

        assert!(confidence >= DEFAULT_CONFIDENCE_FLOOR);
    }
}
