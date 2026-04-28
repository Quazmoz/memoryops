use std::sync::Arc;

use chrono::{DateTime, Datelike, Duration as ChronoDuration, Timelike, Utc, Weekday};
use common::{
    audit::spawn_audit_log,
    error::AppResult,
    models::{
        AuditAction, WorkspaceConfig, DEFAULT_DECAY_HALF_LIFE_DAYS, DEFAULT_PRUNING_THRESHOLD,
    },
    AppError, AppState,
};
use sqlx::FromRow;
use uuid::Uuid;

use crate::{
    embedder::Embedder,
    promoter::{run_promotion_pass, PromoterConfig},
};

const WORKSPACE_PAGE_SIZE: i64 = 500;
const HARD_DELETE_RETENTION_DAYS: i64 = 30;
const MIN_DECAY_HALF_LIFE_DAYS: u32 = 1;
const MAX_DECAY_HALF_LIFE_DAYS: u32 = 3650;
const MIN_PRUNING_THRESHOLD: f32 = 0.01;
const MAX_PRUNING_THRESHOLD: f32 = 0.50;
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

#[derive(Debug, Clone, Copy, PartialEq)]
struct WorkspaceLifecycleSettings {
    workspace_id: Uuid,
    decay_half_life_days: u32,
    pruning_threshold: f32,
}

pub async fn run_scheduler(state: Arc<AppState>) {
    loop {
        let next = next_scheduler_tick_utc();
        let sleep_for = next
            .signed_duration_since(Utc::now())
            .to_std()
            .unwrap_or_default();
        tokio::time::sleep_until(tokio::time::Instant::now() + sleep_for).await;

        let now = Utc::now();
        if now.hour() == 2 {
            if let Err(error) = run_decay_pass(&state).await {
                tracing::error!(error = ?error, "decay pass failed");
            }
            if let Err(error) = run_pruning_pass(&state).await {
                tracing::error!(error = ?error, "pruning pass failed");
            }
            if let Err(error) = run_hard_delete_pass(&state).await {
                tracing::error!(error = ?error, "hard delete pass failed");
            }
        }

        let is_sunday_promotion = now.weekday() == Weekday::Sun && now.hour() == 3;
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
    let next_daily = next_scheduled_utc_after(now, 2);
    let next_promotion = next_sunday_3am_utc_after(now);
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

fn next_sunday_3am_utc_after(now: DateTime<Utc>) -> DateTime<Utc> {
    let mut date = now.date_naive();
    for _ in 0..8 {
        if let Some(candidate_naive) = date.and_hms_opt(3, 0, 0) {
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
    for workspace_id in fetch_all_workspace_ids(&state.db).await? {
        let config = fetch_workspace_promotion_config(&state.db, workspace_id).await?;
        let report = run_promotion_pass(
            &state.db,
            &state.qdrant,
            state.llm_provider.as_ref(),
            state.embedding_provider.as_ref(),
            workspace_id,
            config,
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
    .fetch_all(&state.db)
    .await
    .map_err(AppError::Database)?;

    len_to_u64(updated_ids.len())
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
    let embedder = Embedder::from_state(state);
    let mut total = 0_u64;

    loop {
        let memories = hard_delete_candidates(state, batch_size).await?;
        if memories.is_empty() {
            break;
        }

        for memory in &memories {
            if let Err(error) = embedder.delete_point(memory.id).await {
                tracing::warn!(error = ?error, memory_id = %memory.id, "failed to delete Qdrant point during hard delete pass");
            }
        }

        let ids = memories.iter().map(|memory| memory.id).collect::<Vec<_>>();
        let deleted = sqlx::query_scalar::<_, Uuid>(
            "DELETE FROM memory_units WHERE id = ANY($1) RETURNING id",
        )
        .bind(ids)
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

async fn fetch_all_workspace_ids(pool: &sqlx::PgPool) -> AppResult<Vec<Uuid>> {
    sqlx::query_scalar::<_, Uuid>(
        r#"
        SELECT id
        FROM workspaces
        WHERE deleted_at IS NULL
        ORDER BY id ASC
        "#,
    )
    .fetch_all(pool)
    .await
    .map_err(AppError::Database)
}

async fn fetch_workspace_promotion_config(
    pool: &sqlx::PgPool,
    workspace_id: Uuid,
) -> AppResult<PromoterConfig> {
    #[derive(Debug, FromRow)]
    struct Row {
        promotion_threshold: f64,
        dedup_cosine_threshold: f64,
    }

    let row = sqlx::query_as::<_, Row>(
        r#"
        SELECT promotion_threshold, dedup_cosine_threshold
        FROM workspaces
        WHERE id = $1 AND deleted_at IS NULL
        "#,
    )
    .bind(workspace_id)
    .fetch_optional(pool)
    .await
    .map_err(AppError::Database)?
    .ok_or_else(|| AppError::NotFound {
        resource: format!("workspace:{workspace_id}"),
    })?;

    Ok(PromoterConfig {
        promotion_threshold: row.promotion_threshold as f32,
        dedup_cosine_threshold: row.dedup_cosine_threshold as f32,
        cluster_min_size: 3,
        batch_size: 200,
    })
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
          AND deleted_at < now() - interval '30 days'
        ORDER BY deleted_at ASC, id ASC
        LIMIT $1
        "#,
    )
    .bind(limit)
    .fetch_all(&state.db)
    .await
    .map_err(AppError::Database)
}

fn len_to_u64(value: usize) -> AppResult<u64> {
    u64::try_from(value).map_err(|error| AppError::Internal(anyhow::anyhow!(error)))
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
    }
}

impl WorkspaceLifecycleSettings {
    fn default_for_workspace(workspace_id: Uuid) -> Self {
        Self {
            workspace_id,
            decay_half_life_days: DEFAULT_DECAY_HALF_LIFE_DAYS,
            pruning_threshold: DEFAULT_PRUNING_THRESHOLD,
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
