use chrono::{DateTime, Utc};
use common::{
    error::AppResult,
    models::{DEFAULT_DECAY_HALF_LIFE_DAYS, DEFAULT_PRUNING_THRESHOLD},
    services::WorkspaceConfigService,
    AppState,
};
use uuid::Uuid;

use crate::store;

pub const SECONDS_PER_DAY: f64 = 86_400.0;

pub async fn run_decay_pass(state: &AppState, workspace_id: Uuid) -> AppResult<u64> {
    let config = WorkspaceConfigService::new(state.db.clone())
        .load(workspace_id)
        .await?;
    let half_life_days = config
        .decay_half_life_days
        .unwrap_or(DEFAULT_DECAY_HALF_LIFE_DAYS);
    let pruning_threshold = config
        .pruning_threshold
        .unwrap_or(DEFAULT_PRUNING_THRESHOLD);
    let updated = store::apply_decay_scores_with_half_life(
        &state.db,
        workspace_id,
        half_life_days,
        pruning_threshold,
    )
    .await?;
    tracing::info!(workspace_id = %workspace_id, updated, "applied memory decay scores");
    Ok(updated)
}

pub fn decay_score(importance_score: f32, elapsed_secs: f64, half_life_secs: f64) -> f32 {
    if half_life_secs <= 0.0 {
        return 0.0;
    }

    let score = f64::from(importance_score) * 0.5_f64.powf(elapsed_secs / half_life_secs);
    score.clamp(0.0, 1.0) as f32
}

pub fn decay_score_at(
    importance_score: f64,
    created_at: DateTime<Utc>,
    as_of: DateTime<Utc>,
    half_life_days: f64,
) -> f64 {
    if half_life_days <= 0.0 {
        return 0.0;
    }

    let elapsed_days = (as_of - created_at).num_seconds() as f64 / SECONDS_PER_DAY;
    importance_score * 0.5_f64.powf(elapsed_days / half_life_days)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decay_score_formula_is_correct() {
        let half_life_secs = f64::from(DEFAULT_DECAY_HALF_LIFE_DAYS) * SECONDS_PER_DAY;
        let score = decay_score(1.0, half_life_secs, half_life_secs);

        assert!((score - 0.5).abs() < 0.0001);
    }

    #[test]
    fn decay_score_at_returns_importance_at_creation() {
        let created_at = Utc::now();
        let score = decay_score_at(0.8, created_at, created_at, 30.0);

        assert!((score - 0.8).abs() < 0.0001);
    }

    #[test]
    fn decay_score_at_halves_at_half_life() {
        let created_at = Utc::now();
        let as_of = created_at + chrono::Duration::days(30);
        let score = decay_score_at(0.8, created_at, as_of, 30.0);

        assert!((score - 0.4).abs() < 0.0001);
    }
}
