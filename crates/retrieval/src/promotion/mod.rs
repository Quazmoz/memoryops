pub mod decay;
pub mod eligibility;

use std::collections::HashSet;

use common::{models::WorkspaceConfig, services::WorkspaceConfigService, AppState};
use uuid::Uuid;

use crate::{access, store};

pub async fn check_and_promote(state: AppState, workspace_id: Uuid, memory_ids: Vec<Uuid>) {
    let memory_ids = unique_memory_ids(memory_ids);
    if memory_ids.is_empty() {
        return;
    }

    if let Err(error) = check_many(&state, workspace_id, &memory_ids).await {
        tracing::warn!(error = ?error, workspace_id = %workspace_id, count = memory_ids.len(), "promotion check failed");
    }
}

async fn check_many(
    state: &AppState,
    workspace_id: Uuid,
    memory_ids: &[Uuid],
) -> common::error::AppResult<()> {
    let access_counts = access::get_access_counts(&state.redis, memory_ids).await?;

    if let Err(error) = store::increment_access_counts(&state.db, memory_ids, workspace_id).await {
        tracing::warn!(error = ?error, workspace_id = %workspace_id, count = memory_ids.len(), "failed to increment database access counts");
    }

    let config = WorkspaceConfigService::new(state.db.clone())
        .load(workspace_id)
        .await?;
    let units = store::get_memory_units_by_ids(&state.db, memory_ids, workspace_id).await?;
    let eligible_ids = units
        .iter()
        .filter(|unit| {
            is_eligible(
                unit,
                access_counts.get(&unit.id).copied().unwrap_or(0),
                &config,
            )
        })
        .map(|unit| unit.id)
        .collect::<Vec<_>>();

    store::promote_to_semantic_batch(&state.db, &eligible_ids, workspace_id).await?;
    for memory_id in eligible_ids {
        tracing::info!(memory_id = %memory_id, workspace_id = %workspace_id, "promoted episodic memory to semantic");
    }

    Ok(())
}

fn is_eligible(
    unit: &common::models::MemoryUnit,
    access_count: u64,
    config: &WorkspaceConfig,
) -> bool {
    eligibility::is_eligible_for_promotion(unit, access_count, config)
}

fn unique_memory_ids(memory_ids: Vec<Uuid>) -> Vec<Uuid> {
    let mut seen = HashSet::with_capacity(memory_ids.len());
    memory_ids
        .into_iter()
        .filter(|memory_id| seen.insert(*memory_id))
        .collect()
}
