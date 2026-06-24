pub mod decay;
pub mod eligibility;

use common::AppState;
use uuid::Uuid;

use crate::{access, store};

use common::models::WorkspaceConfig;

pub async fn check_and_promote(
    state: AppState,
    workspace_id: Uuid,
    memory_ids: Vec<Uuid>,
    config: &WorkspaceConfig,
) {
    for memory_id in memory_ids {
        if let Err(error) = check_one(&state, workspace_id, memory_id, config).await {
            tracing::warn!(error = ?error, memory_id = %memory_id, "promotion check failed");
        }
    }
}

async fn check_one(
    state: &AppState,
    workspace_id: Uuid,
    memory_id: Uuid,
    config: &WorkspaceConfig,
) -> common::error::AppResult<()> {
    let access_count = access::get_access_count(&state.redis, memory_id).await?;

    if let Err(error) = store::increment_access_count(&state.db, memory_id, workspace_id).await {
        tracing::warn!(error = ?error, memory_id = %memory_id, "failed to increment database access count");
    }

    let Some(unit) = store::get_memory_unit_by_id(&state.db, memory_id, workspace_id).await? else {
        tracing::debug!(memory_id = %memory_id, workspace_id = %workspace_id, "memory not found during promotion check");
        return Ok(());
    };

    if eligibility::is_eligible_for_promotion(&unit, access_count, config) {
        store::promote_to_semantic(&state.db, memory_id, workspace_id).await?;
        tracing::info!(memory_id = %memory_id, workspace_id = %workspace_id, "promoted episodic memory to semantic");
    }

    Ok(())
}
