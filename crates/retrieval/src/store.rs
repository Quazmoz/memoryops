use anyhow::anyhow;
use chrono::{DateTime, Utc};
use common::{
    error::AppResult,
    models::{Entity, MemoryScope, MemoryType, MemoryUnit},
    AppError,
};
use sqlx::{types::Json, PgPool, Postgres, QueryBuilder};
use uuid::Uuid;

use crate::dto::{
    parse_memory_type, ListQuery, SortDirection, SortField, UpdateMemoryRequest, MAX_LIMIT,
};

const MEMORY_COLUMNS: &str = "id, workspace_id, scope, memory_type, content, entities, importance_score, importance_overridden, source_events, embedding_id, token_count, decay_score, pinned, tags, version, deleted_at, last_accessed_at, created_at, updated_at";
const DEFAULT_DECAY_HALF_LIFE_SECS: f64 = 30.0 * 86_400.0;

#[derive(Debug, sqlx::FromRow)]
struct MemoryUnitWithTotal {
    id: Uuid,
    workspace_id: Uuid,
    scope: MemoryScope,
    memory_type: MemoryType,
    content: String,
    entities: Json<Vec<Entity>>,
    importance_score: f32,
    importance_overridden: bool,
    source_events: Vec<Uuid>,
    embedding_id: Option<String>,
    token_count: Option<i32>,
    decay_score: f32,
    pinned: bool,
    tags: Vec<String>,
    version: i32,
    deleted_at: Option<DateTime<Utc>>,
    last_accessed_at: Option<DateTime<Utc>>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    total_count: i64,
}

impl From<MemoryUnitWithTotal> for MemoryUnit {
    fn from(row: MemoryUnitWithTotal) -> Self {
        Self {
            id: row.id,
            workspace_id: row.workspace_id,
            scope: row.scope,
            memory_type: row.memory_type,
            content: row.content,
            entities: row.entities,
            importance_score: row.importance_score,
            importance_overridden: row.importance_overridden,
            source_events: row.source_events,
            embedding_id: row.embedding_id,
            token_count: row.token_count,
            decay_score: row.decay_score,
            pinned: row.pinned,
            tags: row.tags,
            version: row.version,
            deleted_at: row.deleted_at,
            last_accessed_at: row.last_accessed_at,
            created_at: row.created_at,
            updated_at: row.updated_at,
        }
    }
}

pub async fn get_memory_unit_by_id(
    db: &PgPool,
    id: Uuid,
    workspace_id: Uuid,
) -> AppResult<Option<MemoryUnit>> {
    let sql = format!(
        "SELECT {MEMORY_COLUMNS} FROM memory_units WHERE id = $1 AND workspace_id = $2 AND deleted_at IS NULL"
    );

    sqlx::query_as::<_, MemoryUnit>(&sql)
        .bind(id)
        .bind(workspace_id)
        .fetch_optional(db)
        .await
        .map_err(AppError::Database)
}

pub async fn get_memory_units_by_ids(
    db: &PgPool,
    ids: &[Uuid],
    workspace_id: Uuid,
) -> AppResult<Vec<MemoryUnit>> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }

    let sql = format!(
        "SELECT {MEMORY_COLUMNS} FROM memory_units WHERE workspace_id = $1 AND deleted_at IS NULL AND id = ANY($2)"
    );

    sqlx::query_as::<_, MemoryUnit>(&sql)
        .bind(workspace_id)
        .bind(ids.to_vec())
        .fetch_all(db)
        .await
        .map_err(AppError::Database)
}

pub async fn list_memory_units(
    db: &PgPool,
    params: &ListQuery,
) -> AppResult<(Vec<MemoryUnit>, u64)> {
    let limit = params.resolved_limit().min(MAX_LIMIT);
    let offset = params.resolved_offset();

    let mut builder = QueryBuilder::<Postgres>::new("SELECT ");
    builder.push(MEMORY_COLUMNS);
    builder.push(", COUNT(*) OVER() AS total_count FROM memory_units WHERE workspace_id = ");
    builder.push_bind(params.workspace_id);
    builder.push(" AND deleted_at IS NULL");

    if let Some(memory_type) = &params.memory_type {
        builder.push(" AND memory_type = ");
        builder.push_bind(parse_memory_type(memory_type)?);
    }
    if let Some(pinned) = params.pinned {
        builder.push(" AND pinned = ");
        builder.push_bind(pinned);
    }
    if let Some(min_importance) = params.min_importance {
        builder.push(" AND importance_score >= ");
        builder.push_bind(min_importance);
    }

    builder.push(" ORDER BY ");
    builder.push(sort_column(params.resolved_sort()));
    builder.push(" ");
    builder.push(sort_direction(params.resolved_direction()));
    builder.push(" LIMIT ");
    builder.push_bind(i64::from(limit));
    builder.push(" OFFSET ");
    builder.push_bind(i64::from(offset));

    let rows = builder
        .build_query_as::<MemoryUnitWithTotal>()
        .fetch_all(db)
        .await
        .map_err(AppError::Database)?;
    let total = rows
        .first()
        .map_or(0, |row| nonnegative_i64_to_u64(row.total_count));
    let items = rows.into_iter().map(MemoryUnit::from).collect();

    Ok((items, total))
}

pub async fn touch_last_accessed(db: &PgPool, id: Uuid) -> AppResult<()> {
    sqlx::query("UPDATE memory_units SET last_accessed_at = now() WHERE id = $1")
        .bind(id)
        .execute(db)
        .await
        .map(|_| ())
        .map_err(AppError::Database)
}

pub async fn update_memory_unit(
    db: &PgPool,
    id: Uuid,
    workspace_id: Uuid,
    req: &UpdateMemoryRequest,
) -> AppResult<Option<MemoryUnit>> {
    if let Some(score) = req.importance_score {
        if !(0.0..=1.0).contains(&score) {
            return Err(AppError::Validation(
                "importance_score must be between 0.0 and 1.0".to_owned(),
            ));
        }
    }

    if req.is_empty() {
        return get_memory_unit_by_id(db, id, workspace_id).await;
    }

    let mut builder = QueryBuilder::<Postgres>::new("UPDATE memory_units SET ");
    {
        let mut separated = builder.separated(", ");
        if let Some(pinned) = req.pinned {
            separated.push("pinned = ");
            separated.push_bind(pinned);
        }
        if let Some(importance_score) = req.importance_score {
            separated.push("importance_score = ");
            separated.push_bind(importance_score);
            separated.push("importance_overridden = true");
        }
        if let Some(tags) = &req.tags {
            separated.push("tags = ");
            separated.push_bind(tags.clone());
        }
        separated.push("version = version + 1");
    }

    builder.push(" WHERE id = ");
    builder.push_bind(id);
    builder.push(" AND workspace_id = ");
    builder.push_bind(workspace_id);
    builder.push(" AND deleted_at IS NULL RETURNING ");
    builder.push(MEMORY_COLUMNS);

    builder
        .build_query_as::<MemoryUnit>()
        .fetch_optional(db)
        .await
        .map_err(AppError::Database)
}

pub async fn increment_access_count(db: &PgPool, id: Uuid) -> AppResult<()> {
    sqlx::query("UPDATE memory_units SET access_count = access_count + 1 WHERE id = $1")
        .bind(id)
        .execute(db)
        .await
        .map(|_| ())
        .map_err(AppError::Database)
}

pub async fn promote_to_semantic(db: &PgPool, id: Uuid, workspace_id: Uuid) -> AppResult<()> {
    sqlx::query(
        r#"
        UPDATE memory_units
        SET memory_type = 'semantic', version = version + 1
        WHERE id = $1
          AND workspace_id = $2
          AND memory_type = 'episodic'
          AND deleted_at IS NULL
        "#,
    )
    .bind(id)
    .bind(workspace_id)
    .execute(db)
    .await
    .map(|_| ())
    .map_err(AppError::Database)
}

pub async fn apply_decay_scores(db: &PgPool, workspace_id: Uuid) -> AppResult<u64> {
    apply_decay_scores_with_half_life(db, workspace_id, DEFAULT_DECAY_HALF_LIFE_SECS).await
}

pub async fn apply_decay_scores_with_half_life(
    db: &PgPool,
    workspace_id: Uuid,
    decay_half_life_secs: f64,
) -> AppResult<u64> {
    if decay_half_life_secs <= 0.0 {
        return Err(AppError::Internal(anyhow!(
            "decay half-life seconds must be positive"
        )));
    }

    let updated_ids = sqlx::query_scalar::<_, Uuid>(
        r#"
        UPDATE memory_units
        SET decay_score = GREATEST(
            0.0::double precision,
            importance_score::double precision * POWER(
                0.5::double precision,
                EXTRACT(EPOCH FROM (NOW() - created_at)) / $2
            )
        )::real
        WHERE workspace_id = $1
          AND deleted_at IS NULL
          AND pinned = false
          AND importance_overridden = false
        RETURNING id
        "#,
    )
    .bind(workspace_id)
    .bind(decay_half_life_secs)
    .fetch_all(db)
    .await
    .map_err(AppError::Database)?;

    match u64::try_from(updated_ids.len()) {
        Ok(count) => Ok(count),
        Err(_) => Err(AppError::Internal(anyhow!(
            "updated row count exceeded u64"
        ))),
    }
}

fn sort_column(field: SortField) -> &'static str {
    match field {
        SortField::ImportanceScore => "importance_score",
        SortField::DecayScore => "decay_score",
        SortField::UpdatedAt => "updated_at",
        SortField::CreatedAt => "created_at",
    }
}

fn sort_direction(direction: SortDirection) -> &'static str {
    match direction {
        SortDirection::Asc => "ASC",
        SortDirection::Desc => "DESC",
    }
}

fn nonnegative_i64_to_u64(value: i64) -> u64 {
    if value <= 0 {
        0
    } else {
        value as u64
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    async fn insert_workspace(pool: &PgPool, workspace_id: Uuid) {
        let result = sqlx::query("INSERT INTO workspaces (id, name, config) VALUES ($1, $2, $3)")
            .bind(workspace_id)
            .bind(format!("workspace-{workspace_id}"))
            .bind(json!({}))
            .execute(pool)
            .await;

        if let Err(error) = result {
            panic!("test workspace insert should succeed: {error}");
        }
    }

    async fn insert_memory(
        pool: &PgPool,
        workspace_id: Uuid,
        content: &str,
        importance_score: f32,
    ) -> Uuid {
        let id = Uuid::now_v7();
        let result = sqlx::query(
            r#"
            INSERT INTO memory_units (
                id,
                workspace_id,
                scope,
                memory_type,
                content,
                entities,
                importance_score,
                tags
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            "#,
        )
        .bind(id)
        .bind(workspace_id)
        .bind(json!({
            "workspace_id": workspace_id,
            "agent_id": null,
            "user_id": null,
            "repo": "Quazmoz/memoryops",
            "source": "github"
        }))
        .bind(MemoryType::Episodic)
        .bind(content)
        .bind(json!([]))
        .bind(importance_score)
        .bind(Vec::<String>::new())
        .execute(pool)
        .await;

        if let Err(error) = result {
            panic!("test memory insert should succeed: {error}");
        }

        id
    }

    #[sqlx::test(migrations = "../../migrations")]
    #[ignore = "requires live PostgreSQL from docker-compose.test.yml"]
    async fn get_memory_unit_by_id_returns_none_for_wrong_workspace(pool: PgPool) {
        let workspace_id = Uuid::now_v7();
        let other_workspace_id = Uuid::now_v7();
        insert_workspace(&pool, workspace_id).await;
        insert_workspace(&pool, other_workspace_id).await;
        let memory_id = insert_memory(&pool, workspace_id, "scoped memory", 0.7).await;

        let result = match get_memory_unit_by_id(&pool, memory_id, other_workspace_id).await {
            Ok(result) => result,
            Err(error) => panic!("lookup should succeed: {error}"),
        };

        assert!(result.is_none());
    }

    #[sqlx::test(migrations = "../../migrations")]
    #[ignore = "requires live PostgreSQL from docker-compose.test.yml"]
    async fn list_memory_units_respects_limit_and_offset(pool: PgPool) {
        let workspace_id = Uuid::now_v7();
        insert_workspace(&pool, workspace_id).await;
        insert_memory(&pool, workspace_id, "first", 0.9).await;
        let second_id = insert_memory(&pool, workspace_id, "second", 0.8).await;
        insert_memory(&pool, workspace_id, "third", 0.7).await;

        let params = ListQuery {
            workspace_id,
            limit: Some(1),
            offset: Some(1),
            memory_type: None,
            pinned: None,
            min_importance: None,
            sort: None,
            direction: None,
        };

        let (items, total) = match list_memory_units(&pool, &params).await {
            Ok(result) => result,
            Err(error) => panic!("list should succeed: {error}"),
        };

        assert_eq!(total, 3);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].id, second_id);
    }

    #[sqlx::test(migrations = "../../migrations")]
    #[ignore = "requires live PostgreSQL from docker-compose.test.yml"]
    async fn update_memory_unit_sets_importance_overridden(pool: PgPool) {
        let workspace_id = Uuid::now_v7();
        insert_workspace(&pool, workspace_id).await;
        let memory_id = insert_memory(&pool, workspace_id, "update me", 0.7).await;
        let req = UpdateMemoryRequest {
            pinned: None,
            importance_score: Some(0.95),
            tags: None,
        };

        let updated = match update_memory_unit(&pool, memory_id, workspace_id, &req).await {
            Ok(Some(updated)) => updated,
            Ok(None) => panic!("updated memory should exist"),
            Err(error) => panic!("update should succeed: {error}"),
        };

        assert_eq!(updated.importance_score, 0.95);
        assert!(updated.importance_overridden);
    }

    #[sqlx::test(migrations = "../../migrations")]
    #[ignore = "requires live PostgreSQL from docker-compose.test.yml"]
    async fn promote_to_semantic_changes_memory_type(pool: PgPool) {
        let workspace_id = Uuid::now_v7();
        insert_workspace(&pool, workspace_id).await;
        let memory_id = insert_memory(&pool, workspace_id, "promote me", 0.95).await;

        if let Err(error) = promote_to_semantic(&pool, memory_id, workspace_id).await {
            panic!("promotion should succeed: {error}");
        }

        let promoted = match get_memory_unit_by_id(&pool, memory_id, workspace_id).await {
            Ok(Some(unit)) => unit,
            Ok(None) => panic!("promoted memory should exist"),
            Err(error) => panic!("lookup should succeed: {error}"),
        };

        assert_eq!(promoted.memory_type, MemoryType::Semantic);
    }
}
