pub mod classify;
pub mod fast;
pub mod slow;

use common::{
    error::AppResult,
    models::{MemoryUnit, RawEvent},
    AppState,
};

pub async fn process_event(state: &AppState, event: &RawEvent) -> AppResult<MemoryUnit> {
    if classify::should_use_fast_path(event) {
        fast::run_fast_path(state, event).await
    } else {
        slow::run_slow_path(state, event).await
    }
}
