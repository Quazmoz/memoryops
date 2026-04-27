pub mod dlq;
pub mod embedder;
pub mod extractor;
pub mod pipeline;
pub mod scope;
pub mod store;
pub mod worker;

use common::AppState;

pub use common::models::{Entity, EntityType, MemoryUnit, RawEvent};

pub async fn start_workers(state: AppState) -> anyhow::Result<()> {
    for worker_id in 0..state.config.processor.fast_path_concurrency {
        let worker_state = state.clone();
        tokio::spawn(async move {
            worker::run_worker(worker_id, worker_state).await;
        });
    }

    Ok(())
}
