pub mod classify;
pub mod fast;

use common::{
    error::AppResult,
    models::{MemoryUnit, RawEvent},
    AppState,
};

pub async fn process_event(state: &AppState, event: &RawEvent) -> AppResult<MemoryUnit> {
    if !classify::should_use_fast_path(event) {
        tracing::debug!(
            event_id = %event.id,
            "event will receive slow enrichment after initial memory creation"
        );
    }

    fast::run_fast_path(state, event).await
}
