use std::{
    collections::HashMap,
    sync::{Arc, OnceLock},
};

use axum::{
    extract::{Path, Query, State},
    Extension, Json,
};
use common::{auth::AuthContext, error::AppResult, AppError, AppState};
use sqlx::PgPool;
use tokio::sync::Semaphore;
use uuid::Uuid;

use crate::{access, dto::MemoryUnitDto, store};

use super::{resolve_workspace_id, workspace_id_param};

const LAST_ACCESSED_UPDATE_MAX_IN_FLIGHT: usize = 64;
static LAST_ACCESSED_UPDATE_PERMITS: OnceLock<Arc<Semaphore>> = OnceLock::new();

#[axum::debug_handler]
pub async fn handle_get(
    State(state): State<AppState>,
    auth: Option<Extension<AuthContext>>,
    Path(id): Path<Uuid>,
    Query(params): Query<HashMap<String, String>>,
) -> AppResult<Json<MemoryUnitDto>> {
    let auth_context = auth.as_ref().map(|extension| &extension.0);
    let workspace_id = resolve_workspace_id(auth_context, workspace_id_param(&params)?)?;
    let unit = store::get_memory_unit_by_id(&state.db, id, workspace_id)
        .await?
        .ok_or_else(|| AppError::NotFound {
            resource: format!("memory:{id}"),
        })?;

    if let Err(error) = access::record_access(&state.redis, id).await {
        tracing::warn!(error = ?error, memory_id = %id, "failed to record memory access");
    }

    spawn_touch_last_accessed(state.db.clone(), id, workspace_id);

    Ok(Json(MemoryUnitDto::from(unit)))
}

fn spawn_touch_last_accessed(db: PgPool, id: Uuid, workspace_id: Uuid) {
    let permits = last_accessed_update_permits();
    let Ok(permit) = permits.try_acquire_owned() else {
        tracing::warn!(memory_id = %id, "last_accessed_at update queue is full; dropping update");
        return;
    };

    tokio::spawn(async move {
        let _permit = permit;
        if let Err(error) = store::touch_last_accessed(&db, id, workspace_id).await {
            tracing::warn!(error = ?error, memory_id = %id, "failed to touch last_accessed_at");
        }
    });
}

fn last_accessed_update_permits() -> Arc<Semaphore> {
    LAST_ACCESSED_UPDATE_PERMITS
        .get_or_init(|| Arc::new(Semaphore::new(LAST_ACCESSED_UPDATE_MAX_IN_FLIGHT)))
        .clone()
}
