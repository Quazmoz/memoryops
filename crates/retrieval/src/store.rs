use anyhow::anyhow;
use chrono::{DateTime, Utc};
use common::{
    error::AppResult,
    models::{
        Entity, MemoryScope, MemoryType, MemoryUnit, MemoryVersion, ScopeVisibility,
        WorkspaceConfig, DEFAULT_DECAY_HALF_LIFE_DAYS, DEFAULT_PRUNING_THRESHOLD,
    },
    AppError,
};
use sqlx::{types::Json, PgPool, Postgres, QueryBuilder};
use uuid::Uuid;

use crate::dto::{
    parse_memory_type, ListQuery, ScopeFilter, SortDirection, SortField, UpdateMemoryRequest,
    WorkspacePoolAccess, MAX_LIMIT,
};

const MEMORY_COLUMNS: &str = "id, workspace_id, scope, memory_type, scope_visibility, content, entities, importance_score, importance_overridden, source_events, embedding_id, token_count, decay_score, pinned, tags, version, promoted_at, source_episode_ids, corroboration_count, deleted_at, last_accessed_at, created_at, updated_at";
const SECONDS_PER_DAY: f64 = 86_400.0;

#[derive(Debug, sqlx::FromRow)]
struct MemoryUnitWithTotal {
    id: Uuid,
    workspace_id: Uuid,
    scope: MemoryScope,
    memory_type: MemoryType,
    scope_visibility: ScopeVisibility,
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
    promoted_at: Option<DateTime<Utc>>,
    source_episode_ids: Vec<Uuid>,
    corroboration_count: i32,
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
            scope_visibility: row.scope_visibility,
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
            promoted_at: row.promoted_at,
            source_episode_ids: row.source_episode_ids,
            corroboration_count: row.corroboration_count,
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

pub async fn get_memory_unit_by_id_including_deleted(
    db: &PgPool,
    id: Uuid,
    workspace_id: Uuid,
) -> AppResult<Option<MemoryUnit>> {
    let sql =
        format!("SELECT {MEMORY_COLUMNS} FROM memory_units WHERE id = $1 AND workspace_id = $2");

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

pub async fn get_memory_units_by_ids_at(
    db: &PgPool,
    ids: &[Uuid],
    workspace_id: Uuid,
    as_of: DateTime<Utc>,
) -> AppResult<Vec<MemoryUnit>> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }

    let half_life_days = fetch_workspace_half_life_days(db, workspace_id).await?;
    let mut builder = QueryBuilder::<Postgres>::new("SELECT ");
    push_historical_memory_columns(&mut builder, as_of, half_life_days);
    builder.push(" FROM memory_units m");
    push_historical_version_join(&mut builder, as_of);
    builder.push(" WHERE m.workspace_id = ");
    builder.push_bind(workspace_id);
    builder.push(" AND m.id = ANY(");
    builder.push_bind(ids.to_vec());
    builder.push(")");
    push_as_of_existence_filter(&mut builder, as_of);

    builder
        .build_query_as::<MemoryUnit>()
        .fetch_all(db)
        .await
        .map_err(AppError::Database)
}

pub async fn list_memory_units(
    db: &PgPool,
    params: &ListQuery,
    workspace_id: Uuid,
) -> AppResult<(Vec<MemoryUnit>, u64)> {
    if let Some(as_of) = params.as_of {
        return list_memory_units_at(db, params, workspace_id, as_of).await;
    }

    let limit = params.resolved_limit().min(MAX_LIMIT);
    let offset = params.resolved_offset();

    let mut builder = QueryBuilder::<Postgres>::new("SELECT ");
    builder.push(MEMORY_COLUMNS);
    builder.push(", COUNT(*) OVER() AS total_count FROM memory_units WHERE workspace_id = ");
    builder.push_bind(workspace_id);
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
    push_scope_filter(
        &mut builder,
        &scope_from_list_query(params),
        &WorkspacePoolAccess::default(),
        None,
    );

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

async fn list_memory_units_at(
    db: &PgPool,
    params: &ListQuery,
    workspace_id: Uuid,
    as_of: DateTime<Utc>,
) -> AppResult<(Vec<MemoryUnit>, u64)> {
    let limit = params.resolved_limit().min(MAX_LIMIT);
    let offset = params.resolved_offset();
    let half_life_days = fetch_workspace_half_life_days(db, workspace_id).await?;

    let mut builder = QueryBuilder::<Postgres>::new("SELECT ");
    push_historical_memory_columns(&mut builder, as_of, half_life_days);
    builder.push(", COUNT(*) OVER() AS total_count FROM memory_units m");
    push_historical_version_join(&mut builder, as_of);
    builder.push(" WHERE m.workspace_id = ");
    builder.push_bind(workspace_id);
    push_as_of_existence_filter(&mut builder, as_of);

    if let Some(memory_type) = &params.memory_type {
        builder.push(" AND m.memory_type = ");
        builder.push_bind(parse_memory_type(memory_type)?);
    }
    if let Some(pinned) = params.pinned {
        builder.push(" AND m.pinned = ");
        builder.push_bind(pinned);
    }
    if let Some(min_importance) = params.min_importance {
        builder.push(" AND COALESCE(mv.importance_score, m.importance_score) >= ");
        builder.push_bind(min_importance);
    }
    push_scope_filter(
        &mut builder,
        &scope_from_list_query(params),
        &WorkspacePoolAccess::default(),
        Some("m"),
    );

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
    let mut wrote_assignment = false;
    if let Some(pinned) = req.pinned {
        push_assignment_separator(&mut builder, &mut wrote_assignment);
        builder.push("pinned = ");
        builder.push_bind(pinned);
    }
    if let Some(importance_score) = req.importance_score {
        push_assignment_separator(&mut builder, &mut wrote_assignment);
        builder.push("importance_score = ");
        builder.push_bind(importance_score);
        builder.push(", importance_overridden = true");
    }
    if let Some(tags) = &req.tags {
        push_assignment_separator(&mut builder, &mut wrote_assignment);
        builder.push("tags = ");
        builder.push_bind(tags.clone());
    }
    push_assignment_separator(&mut builder, &mut wrote_assignment);
    builder.push("version = version + 1");

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

pub async fn soft_delete_memory_unit(
    db: &PgPool,
    id: Uuid,
    workspace_id: Uuid,
) -> AppResult<Option<MemoryUnit>> {
    let sql = format!(
        "UPDATE memory_units SET deleted_at = now(), embedding_id = NULL, version = version + 1 WHERE id = $1 AND workspace_id = $2 AND deleted_at IS NULL RETURNING {MEMORY_COLUMNS}"
    );

    sqlx::query_as::<_, MemoryUnit>(&sql)
        .bind(id)
        .bind(workspace_id)
        .fetch_optional(db)
        .await
        .map_err(AppError::Database)
}

pub async fn restore_memory_unit(
    db: &PgPool,
    id: Uuid,
    workspace_id: Uuid,
) -> AppResult<Option<MemoryUnit>> {
    let sql = format!(
        "UPDATE memory_units SET deleted_at = NULL, embedding_id = NULL, version = version + 1 WHERE id = $1 AND workspace_id = $2 AND deleted_at IS NOT NULL RETURNING {MEMORY_COLUMNS}"
    );

    sqlx::query_as::<_, MemoryUnit>(&sql)
        .bind(id)
        .bind(workspace_id)
        .fetch_optional(db)
        .await
        .map_err(AppError::Database)
}

pub async fn force_promote_to_semantic(
    db: &PgPool,
    id: Uuid,
    workspace_id: Uuid,
) -> AppResult<Option<MemoryUnit>> {
    let sql = format!(
        "UPDATE memory_units SET memory_type = 'semantic', version = version + 1 WHERE id = $1 AND workspace_id = $2 AND deleted_at IS NULL RETURNING {MEMORY_COLUMNS}"
    );

    sqlx::query_as::<_, MemoryUnit>(&sql)
        .bind(id)
        .bind(workspace_id)
        .fetch_optional(db)
        .await
        .map_err(AppError::Database)
}

pub async fn publish_memory_unit(
    db: &PgPool,
    id: Uuid,
    workspace_id: Uuid,
) -> AppResult<Option<MemoryUnit>> {
    let sql = format!(
        "UPDATE memory_units SET scope_visibility = 'workspace', version = version + 1 WHERE id = $1 AND workspace_id = $2 AND deleted_at IS NULL RETURNING {MEMORY_COLUMNS}"
    );

    sqlx::query_as::<_, MemoryUnit>(&sql)
        .bind(id)
        .bind(workspace_id)
        .fetch_optional(db)
        .await
        .map_err(AppError::Database)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BulkStoreAction {
    Pin,
    Unpin,
    Delete,
}

pub async fn bulk_update_memory_units(
    db: &PgPool,
    ids: &[Uuid],
    workspace_id: Uuid,
    action: BulkStoreAction,
) -> AppResult<Vec<MemoryUnit>> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }

    let mut transaction = db.begin().await.map_err(AppError::Database)?;
    let sql = match action {
        BulkStoreAction::Pin => format!(
            "UPDATE memory_units SET pinned = true, version = version + 1 WHERE workspace_id = $1 AND id = ANY($2) AND deleted_at IS NULL RETURNING {MEMORY_COLUMNS}"
        ),
        BulkStoreAction::Unpin => format!(
            "UPDATE memory_units SET pinned = false, version = version + 1 WHERE workspace_id = $1 AND id = ANY($2) AND deleted_at IS NULL RETURNING {MEMORY_COLUMNS}"
        ),
        BulkStoreAction::Delete => format!(
            "UPDATE memory_units SET deleted_at = now(), version = version + 1 WHERE workspace_id = $1 AND id = ANY($2) AND deleted_at IS NULL RETURNING {MEMORY_COLUMNS}"
        ),
    };

    let units = sqlx::query_as::<_, MemoryUnit>(&sql)
        .bind(workspace_id)
        .bind(ids.to_vec())
        .fetch_all(&mut *transaction)
        .await
        .map_err(AppError::Database)?;

    if units.len() != ids.len() {
        transaction.rollback().await.map_err(AppError::Database)?;
        return Err(AppError::NotFound {
            resource: "one or more memory ids".to_owned(),
        });
    }

    transaction.commit().await.map_err(AppError::Database)?;
    Ok(units)
}

pub async fn list_memory_versions(
    db: &PgPool,
    id: Uuid,
    workspace_id: Uuid,
) -> AppResult<Vec<MemoryVersion>> {
    sqlx::query_as::<_, MemoryVersion>(
        r#"
        SELECT id, memory_id, workspace_id, version, content, importance_score, tags, edited_by, created_at
        FROM memory_versions
        WHERE memory_id = $1 AND workspace_id = $2
        ORDER BY version DESC
        "#,
    )
    .bind(id)
    .bind(workspace_id)
    .fetch_all(db)
    .await
    .map_err(AppError::Database)
}

#[derive(Debug)]
pub struct MergeResult {
    pub source: MemoryUnit,
    pub target_before: MemoryUnit,
    pub target_after: MemoryUnit,
}

pub async fn merge_memory_units(
    db: &PgPool,
    source_id: Uuid,
    target_id: Uuid,
    workspace_id: Uuid,
    actor: &str,
) -> AppResult<MergeResult> {
    if source_id == target_id {
        return Err(AppError::Validation(
            "source_id and target_id must be different".to_owned(),
        ));
    }

    let mut transaction = db.begin().await.map_err(AppError::Database)?;
    let source = select_memory_for_update(&mut transaction, source_id, workspace_id).await?;
    let target_before = select_memory_for_update(&mut transaction, target_id, workspace_id).await?;

    sqlx::query(
        r#"
        INSERT INTO memory_versions (
            id, memory_id, workspace_id, version, content, importance_score, tags, edited_by
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
        "#,
    )
    .bind(Uuid::now_v7())
    .bind(target_before.id)
    .bind(workspace_id)
    .bind(target_before.version)
    .bind(&target_before.content)
    .bind(target_before.importance_score)
    .bind(&target_before.tags)
    .bind(actor)
    .execute(&mut *transaction)
    .await
    .map_err(AppError::Database)?;

    let merged_content = format!(
        "{}\n\n--- merged memory ---\n\n{}",
        target_before.content, source.content
    );
    let sql = format!(
        "UPDATE memory_units SET content = $3, version = version + 1 WHERE id = $1 AND workspace_id = $2 RETURNING {MEMORY_COLUMNS}"
    );
    let target_after = sqlx::query_as::<_, MemoryUnit>(&sql)
        .bind(target_id)
        .bind(workspace_id)
        .bind(merged_content)
        .fetch_one(&mut *transaction)
        .await
        .map_err(AppError::Database)?;

    sqlx::query(
        "UPDATE memory_units SET deleted_at = now(), version = version + 1 WHERE id = $1 AND workspace_id = $2",
    )
    .bind(source_id)
    .bind(workspace_id)
    .execute(&mut *transaction)
    .await
    .map_err(AppError::Database)?;

    transaction.commit().await.map_err(AppError::Database)?;
    Ok(MergeResult {
        source,
        target_before,
        target_after,
    })
}

async fn select_memory_for_update(
    transaction: &mut sqlx::Transaction<'_, Postgres>,
    id: Uuid,
    workspace_id: Uuid,
) -> AppResult<MemoryUnit> {
    let sql = format!(
        "SELECT {MEMORY_COLUMNS} FROM memory_units WHERE id = $1 AND workspace_id = $2 AND deleted_at IS NULL FOR UPDATE"
    );

    sqlx::query_as::<_, MemoryUnit>(&sql)
        .bind(id)
        .bind(workspace_id)
        .fetch_optional(&mut **transaction)
        .await
        .map_err(AppError::Database)?
        .ok_or_else(|| AppError::NotFound {
            resource: format!("memory:{id}"),
        })
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
    apply_decay_scores_with_half_life(
        db,
        workspace_id,
        DEFAULT_DECAY_HALF_LIFE_DAYS,
        DEFAULT_PRUNING_THRESHOLD,
    )
    .await
}

pub async fn apply_decay_scores_with_half_life(
    db: &PgPool,
    workspace_id: Uuid,
    half_life_days: u32,
    pruning_threshold: f32,
) -> AppResult<u64> {
    if half_life_days == 0 {
        return Err(AppError::Internal(anyhow!(
            "decay half-life days must be positive"
        )));
    }
    if !pruning_threshold.is_finite() || !(0.01..=0.50).contains(&pruning_threshold) {
        return Err(AppError::Internal(anyhow!(
            "pruning threshold must be between 0.01 and 0.50"
        )));
    }

    let decay_half_life_secs = f64::from(half_life_days) * SECONDS_PER_DAY;

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

fn push_assignment_separator(
    builder: &mut QueryBuilder<'_, Postgres>,
    wrote_assignment: &mut bool,
) {
    if *wrote_assignment {
        builder.push(", ");
    }
    *wrote_assignment = true;
}

fn scope_from_list_query(params: &ListQuery) -> ScopeFilter {
    ScopeFilter {
        agent_id: normalized_scope_value(params.agent_id.as_ref()),
        user_id: normalized_scope_value(params.user_id.as_ref()),
        repo: normalized_scope_value(params.repo.as_ref()),
    }
}

fn normalized_scope_value(value: Option<&String>) -> Option<String> {
    let trimmed = value?.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_owned())
    }
}

pub(crate) fn push_scope_filter(
    builder: &mut QueryBuilder<'_, Postgres>,
    scope: &ScopeFilter,
    workspace_pool: &WorkspacePoolAccess,
    table_alias: Option<&'static str>,
) {
    if scope.is_empty() {
        return;
    }

    push_agent_scope_field(builder, scope.agent_id.clone(), workspace_pool, table_alias);
    push_scope_field(builder, "user_id", scope.user_id.clone(), table_alias);
    push_scope_field(builder, "repo", scope.repo.clone(), table_alias);
}

pub(crate) fn scope_matches_workspace_pool(
    unit: &MemoryUnit,
    requested_scope: &ScopeFilter,
    workspace_pool: &WorkspacePoolAccess,
) -> bool {
    if let Some(agent_id) = &requested_scope.agent_id {
        let agent_matches = unit.scope.agent_id.as_ref() == Some(agent_id);
        let workspace_matches = match unit.scope_visibility {
            ScopeVisibility::Workspace if workspace_pool.include_all_workspace => true,
            ScopeVisibility::Workspace => {
                unit.scope.agent_id.as_ref().is_some_and(|memory_agent_id| {
                    workspace_pool
                        .inherited_agent_ids
                        .iter()
                        .any(|agent_id| agent_id == memory_agent_id)
                })
            }
            ScopeVisibility::Private => false,
        };

        if !agent_matches && !workspace_matches {
            return false;
        }
    }

    if let Some(user_id) = &requested_scope.user_id {
        if unit.scope.user_id.as_ref() != Some(user_id) {
            return false;
        }
    }
    if let Some(repo) = &requested_scope.repo {
        if unit.scope.repo.as_ref() != Some(repo) {
            return false;
        }
    }

    true
}

fn push_agent_scope_field(
    builder: &mut QueryBuilder<'_, Postgres>,
    value: Option<String>,
    workspace_pool: &WorkspacePoolAccess,
    table_alias: Option<&'static str>,
) {
    let Some(value) = value else {
        return;
    };

    builder.push(" AND (");
    push_qualified_column(builder, table_alias, "agent_id");
    builder.push(" = ");
    builder.push_bind(value);
    if workspace_pool.include_all_workspace {
        builder.push(" OR ");
        push_qualified_column(builder, table_alias, "scope_visibility");
        builder.push(" = 'workspace'");
    } else if !workspace_pool.inherited_agent_ids.is_empty() {
        builder.push(" OR (");
        push_qualified_column(builder, table_alias, "scope_visibility");
        builder.push(" = 'workspace' AND ");
        push_qualified_column(builder, table_alias, "agent_id");
        builder.push(" = ANY(");
        builder.push_bind(workspace_pool.inherited_agent_ids.clone());
        builder.push("))");
    }
    builder.push(")");
}

fn push_scope_field(
    builder: &mut QueryBuilder<'_, Postgres>,
    column: &'static str,
    value: Option<String>,
    table_alias: Option<&'static str>,
) {
    let Some(value) = value else {
        return;
    };

    builder.push(" AND (");
    push_qualified_column(builder, table_alias, column);
    builder.push(" = ");
    builder.push_bind(value);
    builder.push(")");
}

fn push_qualified_column(
    builder: &mut QueryBuilder<'_, Postgres>,
    table_alias: Option<&'static str>,
    column: &'static str,
) {
    if let Some(alias) = table_alias {
        builder.push(alias);
        builder.push(".");
    }
    builder.push(column);
}

fn push_historical_memory_columns(
    builder: &mut QueryBuilder<'_, Postgres>,
    as_of: DateTime<Utc>,
    half_life_days: f64,
) {
    let half_life_secs = half_life_days * SECONDS_PER_DAY;
    builder.push(
        "m.id, m.workspace_id, m.scope, m.memory_type, m.scope_visibility, COALESCE(mv.content, m.content) AS content, m.entities, COALESCE(mv.importance_score, m.importance_score) AS importance_score, m.importance_overridden, m.source_events, m.embedding_id, m.token_count, ",
    );
    builder.push(
        "GREATEST(0.0::double precision, COALESCE(mv.importance_score, m.importance_score)::double precision * POWER(0.5::double precision, EXTRACT(EPOCH FROM (",
    );
    builder.push_bind(as_of);
    builder.push(" - m.created_at)) / ");
    builder.push_bind(half_life_secs);
    builder.push(
        "))::real AS decay_score, m.pinned, COALESCE(mv.tags, m.tags) AS tags, COALESCE(mv.version, m.version) AS version, m.promoted_at, m.source_episode_ids, m.corroboration_count, m.deleted_at, m.last_accessed_at, m.created_at, m.updated_at",
    );
}

fn push_historical_version_join(builder: &mut QueryBuilder<'_, Postgres>, as_of: DateTime<Utc>) {
    builder.push(
        " LEFT JOIN LATERAL (SELECT version, content, importance_score, tags FROM memory_versions WHERE memory_id = m.id AND workspace_id = m.workspace_id AND created_at <= ",
    );
    builder.push_bind(as_of);
    builder.push(" ORDER BY version DESC LIMIT 1) mv ON true");
}

fn push_as_of_existence_filter(builder: &mut QueryBuilder<'_, Postgres>, as_of: DateTime<Utc>) {
    builder.push(" AND m.created_at <= ");
    builder.push_bind(as_of);
    builder.push(" AND (m.deleted_at IS NULL OR m.deleted_at > ");
    builder.push_bind(as_of);
    builder.push(")");
}

async fn fetch_workspace_half_life_days(db: &PgPool, workspace_id: Uuid) -> AppResult<f64> {
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

    let config = serde_json::from_value::<WorkspaceConfig>(value).unwrap_or_default();
    let half_life_days = config
        .decay_half_life_days
        .map(f64::from)
        .unwrap_or(f64::from(DEFAULT_DECAY_HALF_LIFE_DAYS));

    if half_life_days > 0.0 {
        Ok(half_life_days)
    } else {
        Ok(f64::from(DEFAULT_DECAY_HALF_LIFE_DAYS))
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
        insert_scoped_memory(
            pool,
            workspace_id,
            content,
            importance_score,
            None,
            None,
            Some("Quazmoz/memoryops"),
        )
        .await
    }

    async fn insert_scoped_memory(
        pool: &PgPool,
        workspace_id: Uuid,
        content: &str,
        importance_score: f32,
        agent_id: Option<&str>,
        user_id: Option<&str>,
        repo: Option<&str>,
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
            "agent_id": agent_id,
            "user_id": user_id,
            "repo": repo,
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
            workspace_id: Some(workspace_id),
            limit: Some(1),
            offset: Some(1),
            memory_type: None,
            pinned: None,
            min_importance: None,
            agent_id: None,
            user_id: None,
            repo: None,
            as_of: None,
            sort: None,
            direction: None,
        };

        let (items, total) = match list_memory_units(&pool, &params, workspace_id).await {
            Ok(result) => result,
            Err(error) => panic!("list should succeed: {error}"),
        };

        assert_eq!(total, 3);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].id, second_id);
    }

    #[sqlx::test(migrations = "../../migrations")]
    #[ignore = "requires live PostgreSQL from docker-compose.test.yml"]
    async fn list_memory_units_filters_by_agent_id(pool: PgPool) {
        let workspace_id = Uuid::now_v7();
        insert_workspace(&pool, workspace_id).await;
        let memory_a = insert_scoped_memory(
            &pool,
            workspace_id,
            "agent one memory",
            0.9,
            Some("agent-1"),
            None,
            None,
        )
        .await;
        insert_scoped_memory(
            &pool,
            workspace_id,
            "agent two memory",
            0.8,
            Some("agent-2"),
            None,
            None,
        )
        .await;
        insert_scoped_memory(
            &pool,
            workspace_id,
            "workspace memory",
            0.7,
            None,
            None,
            None,
        )
        .await;

        let params = list_query_with_scope(workspace_id, Some("agent-1"), None, None);
        let (items, total) = match list_memory_units(&pool, &params, workspace_id).await {
            Ok(result) => result,
            Err(error) => panic!("agent scope list should succeed: {error}"),
        };

        assert_eq!(total, 1);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].id, memory_a);
    }

    #[sqlx::test(migrations = "../../migrations")]
    #[ignore = "requires live PostgreSQL from docker-compose.test.yml"]
    async fn list_memory_units_filters_by_repo(pool: PgPool) {
        let workspace_id = Uuid::now_v7();
        insert_workspace(&pool, workspace_id).await;
        let repo_memory = insert_scoped_memory(
            &pool,
            workspace_id,
            "repo scoped memory",
            0.9,
            None,
            None,
            Some("Quazmoz/memoryops"),
        )
        .await;
        insert_scoped_memory(
            &pool,
            workspace_id,
            "workspace memory",
            0.8,
            None,
            None,
            None,
        )
        .await;

        let params = list_query_with_scope(workspace_id, None, None, Some("Quazmoz/memoryops"));
        let (items, total) = match list_memory_units(&pool, &params, workspace_id).await {
            Ok(result) => result,
            Err(error) => panic!("repo scope list should succeed: {error}"),
        };

        assert_eq!(total, 1);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].id, repo_memory);
    }

    #[sqlx::test(migrations = "../../migrations")]
    #[ignore = "requires live PostgreSQL from docker-compose.test.yml"]
    async fn list_memory_units_scope_filter_is_workspace_isolated(pool: PgPool) {
        let workspace_id = Uuid::now_v7();
        let other_workspace_id = Uuid::now_v7();
        insert_workspace(&pool, workspace_id).await;
        insert_workspace(&pool, other_workspace_id).await;
        let scoped_memory = insert_scoped_memory(
            &pool,
            workspace_id,
            "workspace one agent memory",
            0.9,
            Some("agent-x"),
            None,
            None,
        )
        .await;
        insert_scoped_memory(
            &pool,
            other_workspace_id,
            "workspace two agent memory",
            0.8,
            Some("agent-x"),
            None,
            None,
        )
        .await;

        let params = list_query_with_scope(workspace_id, Some("agent-x"), None, None);
        let (items, total) = match list_memory_units(&pool, &params, workspace_id).await {
            Ok(result) => result,
            Err(error) => panic!("workspace-isolated scope list should succeed: {error}"),
        };

        assert_eq!(total, 1);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].id, scoped_memory);
    }

    #[sqlx::test(migrations = "../../migrations")]
    #[ignore = "requires live PostgreSQL from docker-compose.test.yml"]
    async fn list_memory_units_null_scope_filter_returns_all(pool: PgPool) {
        let workspace_id = Uuid::now_v7();
        insert_workspace(&pool, workspace_id).await;
        insert_scoped_memory(
            &pool,
            workspace_id,
            "agent one memory",
            0.9,
            Some("agent-1"),
            None,
            None,
        )
        .await;
        insert_scoped_memory(
            &pool,
            workspace_id,
            "agent two memory",
            0.8,
            Some("agent-2"),
            None,
            Some("Quazmoz/memoryops"),
        )
        .await;
        insert_scoped_memory(
            &pool,
            workspace_id,
            "workspace memory",
            0.7,
            None,
            None,
            None,
        )
        .await;

        let params = list_query_with_scope(workspace_id, None, None, None);
        let (items, total) = match list_memory_units(&pool, &params, workspace_id).await {
            Ok(result) => result,
            Err(error) => panic!("unscoped list should succeed: {error}"),
        };

        assert_eq!(total, 3);
        assert_eq!(items.len(), 3);
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

    fn list_query_with_scope(
        workspace_id: Uuid,
        agent_id: Option<&str>,
        user_id: Option<&str>,
        repo: Option<&str>,
    ) -> ListQuery {
        ListQuery {
            workspace_id: Some(workspace_id),
            limit: Some(20),
            offset: Some(0),
            memory_type: None,
            pinned: None,
            min_importance: None,
            agent_id: agent_id.map(str::to_owned),
            user_id: user_id.map(str::to_owned),
            repo: repo.map(str::to_owned),
            as_of: None,
            sort: None,
            direction: None,
        }
    }
}
