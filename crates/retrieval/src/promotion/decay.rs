use common::{error::AppResult, AppState};
use uuid::Uuid;

use crate::store;

pub const DEFAULT_DECAY_HALF_LIFE_DAYS: f64 = 30.0;
pub const SECONDS_PER_DAY: f64 = 86_400.0;

pub async fn run_decay_pass(state: &AppState, workspace_id: Uuid) -> AppResult<u64> {
    let half_life_secs = DEFAULT_DECAY_HALF_LIFE_DAYS * SECONDS_PER_DAY;
    let updated =
        store::apply_decay_scores_with_half_life(&state.db, workspace_id, half_life_secs).await?;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decay_score_formula_is_correct() {
        let half_life_secs = DEFAULT_DECAY_HALF_LIFE_DAYS * SECONDS_PER_DAY;
        let score = decay_score(1.0, half_life_secs, half_life_secs);

        assert!((score - 0.5).abs() < 0.0001);
    }
}
