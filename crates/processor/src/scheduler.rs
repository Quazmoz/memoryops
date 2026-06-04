use std::collections::HashSet;

use chrono::{DateTime, Datelike, Duration as ChronoDuration, Timelike, Utc, Weekday};
use common::{
    audit::spawn_audit_log,
    build_embedding_provider_for_workspace, build_llm_provider_for_workspace,
    error::AppResult,
    models::{
        AuditAction, WorkspaceConfig, DEFAULT_DECAY_HALF_LIFE_DAYS, DEFAULT_PRUNING_THRESHOLD,
    },
    services::{VectorIndexService, WorkspaceConfigService},
    AppError, AppState,
};
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

use crate::{
    config::fetch_workspace_promotion_config, embedder::COLLECTION_NAME,
    promoter::run_promotion_pass,
};

const WORKSPACE_PAGE_SIZE: i64 = 500;
const HARD_DELETE_RETENTION_DAYS: i64 = 30;
const MIN_DECAY_HALF_LIFE_DAYS: u32 = 1;
const MAX_DECAY_HALF_LIFE_DAYS: u32 = 3650;
const MIN_PRUNING_THRESHOLD: f32 = 0.01;
const MAX_PRUNING_THRESHOLD: f32 = 0.50;
const MIN_SKILL_VERSION_RETENTION_DAYS: u32 = 1;
const MAX_SKILL_VERSION_RETENTION_DAYS: u32 = 3650;
const SECONDS_PER_DAY: f64 = 86_400.0;

#[derive(Debug, Clone, Copy, FromRow)]
struct MemoryIdentity {
    id: Uuid,
    workspace_id: Uuid,
}

#[derive(Debug, Clone, FromRow)]
struct WorkspaceConfigRow {
    id: Uuid,
    config: serde_json::Value,
}

#[derive(Debug, Clone, FromRow)]
struct RetentionMemoryCandidate {
    id: Uuid,
    embedding_id: Option<String>,
    source_events: Vec<Uuid>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct WorkspaceLifecycleSettings {
    workspace_id: Uuid,
    decay_half_life_days: u32,
    pruning_threshold: f32,
    skill_version_retention_days: Option<u32>,
}

pub async fn run_scheduler(state: AppState) {
    loop {
        let maintenance_hour = state.config.processor.maintenance_window_hour_utc.min(23);
        let decay_hour = state.config.processor.decay_window_hour_utc.min(23);
        let next = next_scheduler_tick_after_with_windows(Utc::now(), maintenance_hour, decay_hour);
        let sleep_for = next
            .signed_duration_since(Utc::now())
            .to_std()
            .unwrap_or(std::time::Duration::from_secs(60));
        tokio::time::sleep_until(tokio::time::Instant::now() + sleep_for).await;

        let now = Utc::now();
        if now.hour() == maintenance_hour {
            if let Err(error) = run_decay_pass(&state).await {
                tracing::error!(error = ?error, "decay pass failed");
            }
            if let Err(error) = run_pruning_pass(&state).await {
                tracing::error!(error = ?error, "pruning pass failed");
            }
            if let Err(error) = run_skill_version_prune_pass(&state).await {
                tracing::error!(error = ?error, "skill version prune pass failed");
            }
            if let Err(error) = run_hard_delete_pass(&state).await {
                tracing::error!(error = ?error, "hard delete pass failed");
            }
            if let Err(error) = run_compliance_retention_purge(&state).await {
                tracing::error!(error = ?error, "compliance retention purge failed");
            }
        }

        let is_sunday_promotion = now.weekday() == Weekday::Sun && now.hour() == decay_hour;
        if is_sunday_promotion {
            if let Err(error) = run_scheduled_promotion_pass(&state).await {
                tracing::error!(error = ?error, "promotion pass failed");
            }
        }
    }
}

pub fn next_2am_utc() -> DateTime<Utc> {
    next_scheduled_utc_after(Utc::now(), 2)
}

pub fn next_scheduler_tick_utc() -> DateTime<Utc> {
    next_scheduler_tick_after(Utc::now())
}

pub fn next_scheduler_tick_after(now: DateTime<Utc>) -> DateTime<Utc> {
    next_scheduler_tick_after_with_windows(now, 2, 3)
}

pub fn next_scheduler_tick_after_with_windows(
    now: DateTime<Utc>,
    maintenance_hour_utc: u32,
    decay_hour_utc: u32,
) -> DateTime<Utc> {
    let next_daily = next_scheduled_utc_after(now, maintenance_hour_utc as u8);
    let next_promotion = next_sunday_hour_utc_after(now, decay_hour_utc);
    next_daily.min(next_promotion)
}

pub fn next_scheduled_utc_after(now: DateTime<Utc>, hour_utc: u8) -> DateTime<Utc> {
    let hour = u32::from(hour_utc.min(23));
    let date = now.date_naive();
    let Some(candidate_naive) = date.and_hms_opt(hour, 0, 0) else {
        return now + ChronoDuration::days(1);
    };
    let candidate = DateTime::<Utc>::from_naive_utc_and_offset(candidate_naive, Utc);
    if candidate > now {
        return candidate;
    }

    let next_date = date
        .checked_add_signed(ChronoDuration::days(1))
        .unwrap_or(date);
    match next_date.and_hms_opt(hour, 0, 0) {
        Some(next_naive) => DateTime::<Utc>::from_naive_utc_and_offset(next_naive, Utc),
        None => now + ChronoDuration::days(1),
    }
}

fn next_sunday_hour_utc_after(now: DateTime<Utc>, hour_utc: u32) -> DateTime<Utc> {
    let mut date = now.date_naive();
    let hour = hour_utc.min(23);
    for _ in 0..8 {
        if let Some(candidate_naive) = date.and_hms_opt(hour, 0, 0) {
            let candidate = DateTime::<Utc>::from_naive_utc_and_offset(candidate_naive, Utc);
            if candidate > now && candidate.weekday() == Weekday::Sun {
                return candidate;
            }
        }
        date = date
            .checked_add_signed(ChronoDuration::days(1))
            .unwrap_or(date);
    }

    now + ChronoDuration::days(7)
}

async fn run_scheduled_promotion_pass(state: &AppState) -> AppResult<()> {
    let mut cursor = None;
    let config_service = WorkspaceConfigService::new(state.db.clone());

    loop {
        let workspace_ids =
            list_workspace_ids_after(&state.db, cursor, WORKSPACE_PAGE_SIZE).await?;
        if workspace_ids.is_empty() {
            break;
        }

        for workspace_id in &workspace_ids {
            let promotion_config =
                fetch_workspace_promotion_config(&state.db, *workspace_id).await?;
            let workspace_config = config_service.load(*workspace_id).await?;
            let llm_provider = build_llm_provider_for_workspace(&state.config, &workspace_config);
            let embedding_provider =
                build_embedding_provider_for_workspace(&state.config, &workspace_config);
            let report = run_promotion_pass(
                &state.db,
                &state.qdrant,
                llm_provider.as_ref(),
                embedding_provider.as_ref(),
                *workspace_id,
                promotion_config,
            )
            .await
            .map_err(AppError::Internal)?;
            tracing::info!(
                workspace_id = %report.workspace_id,
                clusters_found = report.clusters_found,
                units_promoted = report.units_promoted,
                "promotion pass complete"
            );
        }

        cursor = workspace_ids.last().copied();
    }

    Ok(())
}

pub async fn run_decay_pass(state: &AppState) -> AppResult<u64> {
    let mut total = 0_u64;
    let mut cursor = None;

    loop {
        let workspaces =
            list_workspace_lifecycle_settings_after(state, cursor, WORKSPACE_PAGE_SIZE).await?;
        if workspaces.is_empty() {
            break;
        }

        for workspace in &workspaces {
            total = total.saturating_add(
                apply_decay_scores(
                    state,
                    workspace.workspace_id,
                    workspace.decay_half_life_days,
                )
                .await?,
            );
        }
        cursor = workspaces.last().map(|workspace| workspace.workspace_id);
    }

    tracing::info!(updated = total, "scheduler decay pass completed");
    Ok(total)
}

async fn apply_decay_scores(
    state: &AppState,
    workspace_id: Uuid,
    decay_half_life_days: u32,
) -> AppResult<u64> {
    let decay_half_life_secs = half_life_secs(decay_half_life_days);
    let batch_size = i64::from(state.config.decay.batch_size.max(1));
    let mut cursor = None;
    let mut total = 0_u64;

    loop {
        let updated_ids = decay_score_batch(
            &state.db,
            workspace_id,
            decay_half_life_secs,
            cursor,
            batch_size,
        )
        .await?;
        if updated_ids.is_empty() {
            break;
        }

        cursor = updated_ids.last().copied();
        total = total.saturating_add(len_to_u64(updated_ids.len())?);
    }

    Ok(total)
}

async fn decay_score_batch(
    db: &PgPool,
    workspace_id: Uuid,
    decay_half_life_secs: f64,
    cursor: Option<Uuid>,
    batch_size: i64,
) -> AppResult<Vec<Uuid>> {
    sqlx::query_scalar::<_, Uuid>(
        r#"
        WITH candidates AS (
            SELECT id
            FROM memory_units
            WHERE workspace_id = $1
              AND deleted_at IS NULL
              AND pinned = false
              AND importance_overridden = false
              AND ($3::uuid IS NULL OR id > $3)
            ORDER BY id ASC
            LIMIT $4
            FOR UPDATE SKIP LOCKED
        )
        UPDATE memory_units
        SET decay_score = GREATEST(
            0.0::double precision,
            importance_score::double precision * POWER(
                0.5::double precision,
                EXTRACT(EPOCH FROM (NOW() - created_at)) / $2
            )
        )::real
        FROM candidates
        WHERE memory_units.id = candidates.id
        RETURNING memory_units.id
        "#,
    )
    .bind(workspace_id)
    .bind(decay_half_life_secs)
    .bind(cursor)
    .bind(batch_size)
    .fetch_all(db)
    .await
    .map_err(AppError::Database)
}

pub async fn run_pruning_pass(state: &AppState) -> AppResult<u64> {
    let batch_size = i64::from(state.config.decay.batch_size.max(1));
    let mut total = 0_u64;
    let mut cursor = None;

    loop {
        let workspaces =
            list_workspace_lifecycle_settings_after(state, cursor, WORKSPACE_PAGE_SIZE).await?;
        if workspaces.is_empty() {
            break;
        }

        for workspace in &workspaces {
            loop {
                let pruned = prune_batch(
                    state,
                    workspace.workspace_id,
                    workspace.pruning_threshold,
                    batch_size,
                )
                .await?;
                if pruned.is_empty() {
                    break;
                }

                for memory in &pruned {
                    spawn_audit_log(
                        state.db.clone(),
                        memory.workspace_id,
                        "scheduler".to_owned(),
                        AuditAction::MemoryDeleted,
                        memory.id,
                        "memory",
                        Some(serde_json::json!({ "reason": "decay_pruned" })),
                    );
                }
                total = total.saturating_add(len_to_u64(pruned.len())?);
            }
        }

        cursor = workspaces.last().map(|workspace| workspace.workspace_id);
    }

    tracing::info!(pruned = total, "scheduler pruning pass completed");
    Ok(total)
}

pub async fn run_hard_delete_pass(state: &AppState) -> AppResult<u64> {
    let batch_size = i64::from(state.config.decay.batch_size.max(1));
    let mut total = 0_u64;

    loop {
        let memories = hard_delete_candidates(state, batch_size).await?;
        if memories.is_empty() {
            break;
        }

        let ids = memories.iter().map(|memory| memory.id).collect::<Vec<_>>();
        delete_vectors_best_effort(state, &ids, "hard delete pass").await;
        let deleted = sqlx::query_scalar::<_, Uuid>(
            "DELETE FROM memory_units WHERE id = ANY($1) RETURNING id",
        )
        .bind(&ids)
        .fetch_all(&state.db)
        .await
        .map_err(AppError::Database)?;
        let deleted_count = deleted.len();

        for memory in &memories {
            spawn_audit_log(
                state.db.clone(),
                memory.workspace_id,
                "scheduler".to_owned(),
                AuditAction::MemoryHardDeleted,
                memory.id,
                "memory",
                Some(serde_json::json!({ "reason": "retention_expired" })),
            );
        }

        total = total.saturating_add(len_to_u64(deleted_count)?);
    }

    tracing::info!(deleted = total, "scheduler hard delete pass completed");
    Ok(total)
}

pub async fn run_skill_version_prune_pass(state: &AppState) -> AppResult<u64> {
    let batch_size = i64::from(state.config.decay.batch_size.max(1));
    let mut total = 0_u64;
    let mut cursor = None;

    loop {
        let workspaces =
            list_workspace_lifecycle_settings_after(state, cursor, WORKSPACE_PAGE_SIZE).await?;
        if workspaces.is_empty() {
            break;
        }

        for workspace in &workspaces {
            let Some(retention_days) = workspace.skill_version_retention_days else {
                continue;
            };

            loop {
                let pruned = prune_skill_version_batch(
                    &state.db,
                    workspace.workspace_id,
                    retention_days_to_i32(retention_days)?,
                    batch_size,
                )
                .await?;
                if pruned == 0 {
                    break;
                }

                total = total.saturating_add(pruned);
            }
        }

        cursor = workspaces.last().map(|workspace| workspace.workspace_id);
    }

    tracing::info!(pruned = total, "scheduler skill version prune pass completed");
    Ok(total)
}

pub async fn run_compliance_retention_purge(state: &AppState) -> Result<(), AppError> {
    let batch_size = i64::from(state.config.decay.batch_size.max(1));
    let mut cursor = None;

    loop {
        let workspaces =
            list_retention_workspaces_after(&state.db, cursor, WORKSPACE_PAGE_SIZE).await?;
        if workspaces.is_empty() {
            break;
        }

        for workspace in &workspaces {
            let config = serde_json::from_value::<WorkspaceConfig>(workspace.config.clone())
                .map_err(|error| AppError::Internal(anyhow::anyhow!(error)))?;
            let Some(retention_max_age_days) = config.retention_max_age_days else {
                continue;
            };
            if retention_max_age_days == 0 {
                tracing::warn!(
                    workspace_id = %workspace.id,
                    "workspace retention limit is zero; skipping compliance purge"
                );
                continue;
            }

            let retention_days = retention_days_to_i32(retention_max_age_days)?;
            let (memories_purged, raw_events_purged) = purge_retention_memory_batches(
                state,
                workspace.id,
                retention_days,
                config.compliance_hard_purge,
                batch_size,
            )
            .await?;

            if memories_purged > 0 {
                insert_retention_compliance_audit_log(
                    &state.db,
                    workspace.id,
                    memories_purged,
                    raw_events_purged,
                )
                .await?;
            }

            tracing::info!(
                workspace = %workspace.id,
                memories_purged,
                "compliance purge"
            );
        }

        cursor = workspaces.last().map(|workspace| workspace.id);
    }

    Ok(())
}

async fn list_retention_workspaces_after(
    pool: &PgPool,
    cursor: Option<Uuid>,
    limit: i64,
) -> AppResult<Vec<WorkspaceConfigRow>> {
    sqlx::query_as::<_, WorkspaceConfigRow>(
        r#"
        SELECT id, config
        FROM workspaces
        WHERE deleted_at IS NULL
          AND config->>'retention_max_age_days' IS NOT NULL
          AND ($1::uuid IS NULL OR id > $1)
        ORDER BY id ASC
        LIMIT $2
        "#,
    )
    .bind(cursor)
    .bind(limit)
    .fetch_all(pool)
    .await
    .map_err(AppError::Database)
}

async fn purge_retention_memory_batches(
    state: &AppState,
    workspace_id: Uuid,
    retention_days: i32,
    hard_purge_raw_events: bool,
    batch_size: i64,
) -> AppResult<(u64, u64)> {
    let mut cursor = None;
    let mut memories_purged = 0_u64;
    let mut raw_events_purged = 0_u64;

    loop {
        let candidates = collect_retention_memory_candidates_after(
            &state.db,
            workspace_id,
            retention_days,
            cursor,
            batch_size,
        )
        .await?;
        if candidates.is_empty() {
            break;
        }

        cursor = candidates.last().map(|candidate| candidate.id);
        let ids = candidates
            .iter()
            .map(|candidate| candidate.id)
            .collect::<Vec<_>>();
        let vector_ids = candidates
            .iter()
            .filter(|candidate| candidate.embedding_id.is_some())
            .map(|candidate| candidate.id)
            .collect::<Vec<_>>();
        delete_vectors_best_effort(state, &vector_ids, "compliance retention purge").await;

        let source_event_ids = if hard_purge_raw_events {
            retention_source_event_ids_from_candidates(&candidates)
        } else {
            Vec::new()
        };

        memories_purged = memories_purged
            .saturating_add(delete_retention_memory_batch(&state.db, workspace_id, &ids).await?);
        raw_events_purged = raw_events_purged.saturating_add(
            delete_retention_raw_events(&state.db, workspace_id, &source_event_ids).await?,
        );
    }

    Ok((memories_purged, raw_events_purged))
}

async fn list_workspace_lifecycle_settings_after(
    state: &AppState,
    cursor: Option<Uuid>,
    limit: i64,
) -> AppResult<Vec<WorkspaceLifecycleSettings>> {
    let rows = sqlx::query_as::<_, WorkspaceConfigRow>(
        r#"
        SELECT id, config
        FROM workspaces
        WHERE deleted_at IS NULL
          AND ($1::uuid IS NULL OR id > $1)
        ORDER BY id ASC
        LIMIT $2
        "#,
    )
    .bind(cursor)
    .bind(limit)
    .fetch_all(&state.db)
    .await
    .map_err(AppError::Database)?;

    Ok(rows
        .iter()
        .map(|row| lifecycle_settings_from_value(row.id, &row.config))
        .collect())
}

async fn list_workspace_ids_after(
    pool: &sqlx::PgPool,
    cursor: Option<Uuid>,
    limit: i64,
) -> AppResult<Vec<Uuid>> {
    sqlx::query_scalar::<_, Uuid>(
        r#"
        SELECT id
        FROM workspaces
        WHERE deleted_at IS NULL
          AND ($1::uuid IS NULL OR id > $1)
        ORDER BY id ASC
        LIMIT $2
        "#,
    )
    .bind(cursor)
    .bind(limit)
    .fetch_all(pool)
    .await
    .map_err(AppError::Database)
}

async fn prune_batch(
    state: &AppState,
    workspace_id: Uuid,
    pruning_threshold: f32,
    limit: i64,
) -> AppResult<Vec<MemoryIdentity>> {
    sqlx::query_as::<_, MemoryIdentity>(
        r#"
        WITH candidates AS (
            SELECT id
            FROM memory_units
                        WHERE workspace_id = $1
                            AND decay_score < $2
              AND pinned = false
              AND importance_overridden = false
              AND deleted_at IS NULL
            ORDER BY decay_score ASC, id ASC
                        LIMIT $3
        )
        UPDATE memory_units
        SET deleted_at = now(), version = version + 1
        WHERE id IN (SELECT id FROM candidates)
        RETURNING id, workspace_id
        "#,
    )
    .bind(workspace_id)
    .bind(pruning_threshold)
    .bind(limit)
    .fetch_all(&state.db)
    .await
    .map_err(AppError::Database)
}

async fn hard_delete_candidates(state: &AppState, limit: i64) -> AppResult<Vec<MemoryIdentity>> {
    sqlx::query_as::<_, MemoryIdentity>(
        r#"
        SELECT id, workspace_id
        FROM memory_units
        WHERE deleted_at IS NOT NULL
                    AND deleted_at < now() - ($1 * interval '1 day')
        ORDER BY deleted_at ASC, id ASC
                LIMIT $2
        "#,
    )
    .bind(HARD_DELETE_RETENTION_DAYS)
    .bind(limit)
    .fetch_all(&state.db)
    .await
    .map_err(AppError::Database)
}

async fn collect_retention_memory_candidates_after(
    pool: &PgPool,
    workspace_id: Uuid,
    retention_days: i32,
    cursor: Option<Uuid>,
    limit: i64,
) -> AppResult<Vec<RetentionMemoryCandidate>> {
    sqlx::query_as::<_, RetentionMemoryCandidate>(
        r#"
        SELECT id, embedding_id, COALESCE(source_events, ARRAY[]::uuid[]) AS source_events
        FROM memory_units
        WHERE workspace_id = $1
          AND created_at < NOW() - ($2::INTEGER * INTERVAL '1 day')
          AND pinned = false
          AND importance_overridden = false
          AND hard_deleted_at IS NULL
          AND ($3::uuid IS NULL OR id > $3)
        ORDER BY id ASC
        LIMIT $4
        "#,
    )
    .bind(workspace_id)
    .bind(retention_days)
    .bind(cursor)
    .bind(limit)
    .fetch_all(pool)
    .await
    .map_err(AppError::Database)
}

async fn delete_retention_memory_batch(
    pool: &PgPool,
    workspace_id: Uuid,
    ids: &[Uuid],
) -> AppResult<u64> {
    if ids.is_empty() {
        return Ok(0);
    }

    sqlx::query(
        r#"
        DELETE FROM memory_units
        WHERE workspace_id = $1
          AND id = ANY($2)
        "#,
    )
    .bind(workspace_id)
    .bind(ids)
    .execute(pool)
    .await
    .map(|result| result.rows_affected())
    .map_err(AppError::Database)
}

fn retention_source_event_ids_from_candidates(
    candidates: &[RetentionMemoryCandidate],
) -> Vec<Uuid> {
    let mut seen = HashSet::new();
    let mut ids = Vec::new();
    for source_event_id in candidates
        .iter()
        .flat_map(|candidate| candidate.source_events.iter().copied())
    {
        if seen.insert(source_event_id) {
            ids.push(source_event_id);
        }
    }
    ids
}

async fn delete_retention_raw_events(
    pool: &PgPool,
    workspace_id: Uuid,
    source_event_ids: &[Uuid],
) -> AppResult<u64> {
    if source_event_ids.is_empty() {
        return Ok(0);
    }

    sqlx::query(
        r#"
        DELETE FROM raw_events
        WHERE workspace_id = $1 AND id = ANY($2)
        "#,
    )
    .bind(workspace_id)
    .bind(source_event_ids)
    .execute(pool)
    .await
    .map(|result| result.rows_affected())
    .map_err(AppError::Database)
}

async fn insert_retention_compliance_audit_log(
    pool: &PgPool,
    workspace_id: Uuid,
    memories_purged: u64,
    raw_events_purged: u64,
) -> AppResult<()> {
    // Cast to i64 for BIGINT binding; saturate rather than error on overflow.
    let memories_purged_i64 = i64::try_from(memories_purged).unwrap_or(i64::MAX);
    let raw_events_purged_i64 = i64::try_from(raw_events_purged).unwrap_or(i64::MAX);

    sqlx::query(
        r#"
        INSERT INTO compliance_audit_log (
            workspace_id,
            action,
            target_user_id,
            memories_purged,
            raw_events_purged,
            initiated_by
        )
        VALUES ($1, 'retention_purge', NULL, $2, $3, 'scheduler')
        "#,
    )
    .bind(workspace_id)
    .bind(memories_purged_i64)
    .bind(raw_events_purged_i64)
    .execute(pool)
    .await
    .map(|_| ())
    .map_err(AppError::Database)
}

fn retention_days_to_i32(value: u32) -> AppResult<i32> {
    i32::try_from(value).map_err(|error| AppError::Internal(anyhow::anyhow!(error)))
}

fn len_to_u64(value: usize) -> AppResult<u64> {
    u64::try_from(value).map_err(|error| AppError::Internal(anyhow::anyhow!(error)))
}

async fn delete_vectors_best_effort(state: &AppState, memory_ids: &[Uuid], context: &'static str) {
    if memory_ids.is_empty() {
        return;
    }

    let vector_index = VectorIndexService::new(&state.qdrant, COLLECTION_NAME);
    if let Err(error) = vector_index.delete_points(memory_ids.iter().copied()).await {
        tracing::warn!(
            error = ?error,
            count = memory_ids.len(),
            context,
            "failed to delete vector points"
        );
    }
}

pub fn decay_filter_allows_update(
    pinned: bool,
    importance_overridden: bool,
    deleted: bool,
) -> bool {
    !pinned && !importance_overridden && !deleted
}

pub fn should_prune(
    decay_score: f32,
    pruning_threshold: f32,
    pinned: bool,
    importance_overridden: bool,
    deleted: bool,
) -> bool {
    decay_score < pruning_threshold
        && decay_filter_allows_update(pinned, importance_overridden, deleted)
}

fn lifecycle_settings_from_value(
    workspace_id: Uuid,
    config_value: &serde_json::Value,
) -> WorkspaceLifecycleSettings {
    match serde_json::from_value::<WorkspaceConfig>(config_value.clone()) {
        Ok(config) => lifecycle_settings_from_config(workspace_id, &config),
        Err(error) => {
            tracing::warn!(
                workspace_id = %workspace_id,
                error = ?error,
                "failed to parse workspace config; using lifecycle defaults"
            );
            WorkspaceLifecycleSettings::default_for_workspace(workspace_id)
        }
    }
}

fn lifecycle_settings_from_config(
    workspace_id: Uuid,
    config: &WorkspaceConfig,
) -> WorkspaceLifecycleSettings {
    let decay_half_life_days = config
        .decay_half_life_days
        .unwrap_or(DEFAULT_DECAY_HALF_LIFE_DAYS);
    let pruning_threshold = config
        .pruning_threshold
        .unwrap_or(DEFAULT_PRUNING_THRESHOLD);

    WorkspaceLifecycleSettings {
        workspace_id,
        decay_half_life_days: valid_decay_half_life_days(decay_half_life_days, workspace_id),
        pruning_threshold: valid_pruning_threshold(pruning_threshold, workspace_id),
        skill_version_retention_days: valid_skill_version_retention_days(
            config.skill_version_retention_days,
            workspace_id,
        ),
    }
}

impl WorkspaceLifecycleSettings {
    fn default_for_workspace(workspace_id: Uuid) -> Self {
        Self {
            workspace_id,
            decay_half_life_days: DEFAULT_DECAY_HALF_LIFE_DAYS,
            pruning_threshold: DEFAULT_PRUNING_THRESHOLD,
            skill_version_retention_days: None,
        }
    }
}

fn valid_decay_half_life_days(days: u32, workspace_id: Uuid) -> u32 {
    if (MIN_DECAY_HALF_LIFE_DAYS..=MAX_DECAY_HALF_LIFE_DAYS).contains(&days) {
        days
    } else {
        tracing::warn!(
            workspace_id = %workspace_id,
            decay_half_life_days = days,
            "workspace decay half-life is out of range; using default"
        );
        DEFAULT_DECAY_HALF_LIFE_DAYS
    }
}

fn valid_pruning_threshold(threshold: f32, workspace_id: Uuid) -> f32 {
    if threshold.is_finite() && (MIN_PRUNING_THRESHOLD..=MAX_PRUNING_THRESHOLD).contains(&threshold)
    {
        threshold
    } else {
        tracing::warn!(
            workspace_id = %workspace_id,
            pruning_threshold = threshold,
            "workspace pruning threshold is out of range; using default"
        );
        DEFAULT_PRUNING_THRESHOLD
    }
}

fn valid_skill_version_retention_days(days: Option<u32>, workspace_id: Uuid) -> Option<u32> {
    let Some(days) = days else {
        return None;
    };

    if (MIN_SKILL_VERSION_RETENTION_DAYS..=MAX_SKILL_VERSION_RETENTION_DAYS).contains(&days) {
        Some(days)
    } else {
        tracing::warn!(
            workspace_id = %workspace_id,
            skill_version_retention_days = days,
            "workspace skill version retention is out of range; disabling pruning"
        );
        None
    }
}

fn half_life_secs(decay_half_life_days: u32) -> f64 {
    f64::from(decay_half_life_days) * SECONDS_PER_DAY
}

pub fn hard_delete_eligible(deleted_at: Option<DateTime<Utc>>, now: DateTime<Utc>) -> bool {
    deleted_at.is_some_and(|deleted_at| {
        deleted_at < now - ChronoDuration::days(HARD_DELETE_RETENTION_DAYS)
    })
}

pub fn decay_score(importance_score: f32, elapsed_secs: f64, half_life_secs: f64) -> f32 {
    if half_life_secs <= 0.0 {
        return 0.0;
    }

    let score = f64::from(importance_score) * 0.5_f64.powf(elapsed_secs / half_life_secs);
    score.clamp(0.0, 1.0) as f32
}

async fn prune_skill_version_batch(
    pool: &PgPool,
    workspace_id: Uuid,
    retention_days: i32,
    limit: i64,
) -> AppResult<u64> {
    let deleted = sqlx::query_scalar::<_, Uuid>(
        r#"
        WITH ranked AS (
            SELECT id
            FROM (
                SELECT
                    id,
                    ROW_NUMBER() OVER (
                        PARTITION BY tool_id
                        ORDER BY version DESC, created_at DESC, id DESC
                    ) AS version_rank
                FROM workspace_tool_versions
                WHERE workspace_id = $1
                  AND created_at < NOW() - ($2::INTEGER * INTERVAL '1 day')
            ) candidates
            WHERE version_rank > 1
            ORDER BY id ASC
            LIMIT $3
        )
        DELETE FROM workspace_tool_versions
        WHERE id IN (SELECT id FROM ranked)
        RETURNING id
        "#,
    )
    .bind(workspace_id)
    .bind(retention_days)
    .bind(limit)
    .fetch_all(pool)
    .await
    .map_err(AppError::Database)?;

    len_to_u64(deleted.len())
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;

    use super::*;

    #[test]
    fn decay_boundary_at_exactly_half_life_is_half_importance() {
        let half_life_secs = half_life_secs(DEFAULT_DECAY_HALF_LIFE_DAYS);
        let score = decay_score(1.0, half_life_secs, half_life_secs);

        assert!((score - 0.5).abs() <= 0.001);
    }

    #[test]
    fn decay_filter_skips_pinned_and_overridden_memories() {
        assert!(decay_filter_allows_update(false, false, false));
        assert!(!decay_filter_allows_update(true, false, false));
        assert!(!decay_filter_allows_update(false, true, false));
        assert!(!decay_filter_allows_update(false, false, true));
    }

    #[test]
    fn pruning_threshold_boundary_is_strict() {
        assert!(should_prune(
            0.099,
            DEFAULT_PRUNING_THRESHOLD,
            false,
            false,
            false
        ));
        assert!(!should_prune(
            0.10,
            DEFAULT_PRUNING_THRESHOLD,
            false,
            false,
            false
        ));
        assert!(!should_prune(
            0.101,
            DEFAULT_PRUNING_THRESHOLD,
            false,
            false,
            false
        ));
    }

    #[test]
    fn pruning_skips_pinned_memory() {
        assert!(!should_prune(
            0.01,
            DEFAULT_PRUNING_THRESHOLD,
            true,
            false,
            false
        ));
    }

    #[test]
    fn scheduler_uses_workspace_specific_half_life_when_present() {
        let workspace_id = Uuid::nil();
        let config = WorkspaceConfig {
            decay_half_life_days: Some(7),
            ..WorkspaceConfig::default()
        };

        let settings = lifecycle_settings_from_config(workspace_id, &config);

        assert_eq!(settings.decay_half_life_days, 7);
        assert_eq!(
            half_life_secs(settings.decay_half_life_days),
            7.0 * SECONDS_PER_DAY
        );
    }

    #[test]
    fn scheduler_falls_back_to_default_half_life_when_missing() {
        let settings = lifecycle_settings_from_config(Uuid::nil(), &WorkspaceConfig::default());

        assert_eq!(settings.decay_half_life_days, DEFAULT_DECAY_HALF_LIFE_DAYS);
    }

    #[test]
    fn scheduler_falls_back_to_default_pruning_threshold_when_missing() {
        let settings = lifecycle_settings_from_config(Uuid::nil(), &WorkspaceConfig::default());

        assert_eq!(settings.pruning_threshold, DEFAULT_PRUNING_THRESHOLD);
    }

    #[test]
    fn scheduler_uses_skill_version_retention_when_present() {
        let settings = lifecycle_settings_from_config(
            Uuid::nil(),
            &WorkspaceConfig {
                skill_version_retention_days: Some(45),
                ..WorkspaceConfig::default()
            },
        );

        assert_eq!(settings.skill_version_retention_days, Some(45));
    }

    #[test]
    fn scheduler_disables_skill_version_retention_when_missing() {
        let settings = lifecycle_settings_from_config(Uuid::nil(), &WorkspaceConfig::default());

        assert_eq!(settings.skill_version_retention_days, None);
    }

    #[test]
    fn hard_delete_requires_more_than_thirty_days() {
        let Some(now) = Utc.with_ymd_and_hms(2026, 4, 27, 12, 0, 0).single() else {
            panic!("test timestamp should be valid");
        };
        assert!(hard_delete_eligible(
            Some(now - ChronoDuration::days(31)),
            now
        ));
        assert!(!hard_delete_eligible(
            Some(now - ChronoDuration::days(30)),
            now
        ));
        assert!(!hard_delete_eligible(None, now));
    }

    #[test]
    fn hard_delete_eligible_requires_strictly_more_than_thirty_days() {
        let Some(now) = Utc.with_ymd_and_hms(2026, 4, 27, 12, 0, 0).single() else {
            panic!("test timestamp should be valid");
        };

        assert!(!hard_delete_eligible(
            Some(now - ChronoDuration::days(30)),
            now
        ));
        assert!(hard_delete_eligible(
            Some(now - ChronoDuration::days(30) - ChronoDuration::seconds(1)),
            now
        ));
    }

    #[test]
    fn next_schedule_rolls_to_next_day_after_two_am() {
        let Some(now) = Utc.with_ymd_and_hms(2026, 4, 27, 3, 0, 0).single() else {
            panic!("test timestamp should be valid");
        };
        let next = next_scheduled_utc_after(now, 2);

        assert_eq!(next.date_naive().to_string(), "2026-04-28");
        assert_eq!(next.time().to_string(), "02:00:00");
    }

    #[test]
    fn next_scheduler_tick_picks_sunday_three_am() {
        let Some(now) = Utc.with_ymd_and_hms(2026, 5, 3, 2, 30, 0).single() else {
            panic!("test timestamp should be valid");
        };
        let next = next_scheduler_tick_after(now);

        assert_eq!(next.weekday(), Weekday::Sun);
        assert_eq!(next.hour(), 3);
    }
}
