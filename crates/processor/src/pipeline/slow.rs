use common::{error::AppResult, models::MemoryUnit, models::RawEvent, AppState};

use super::fast;

pub async fn run_slow_path(state: &AppState, event: &RawEvent) -> AppResult<MemoryUnit> {
    fast::run_fast_path(state, event).await
}
