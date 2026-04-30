use std::collections::HashMap;

use axum::{
    extract::{Path, Query, State},
    Extension, Json,
};
use common::{auth::AuthContext, error::AppResult, AppError, AppState};
use uuid::Uuid;

use crate::{access, dto::MemoryUnitDto, store};

use super::{resolve_workspace_id, workspace_id_param};

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

    let db = state.db.clone();
    tokio::spawn(async move {
        if let Err(error) = store::touch_last_accessed(&db, id, workspace_id).await {
            tracing::warn!(error = ?error, memory_id = %id, "failed to touch last_accessed_at");
        }
    });

    Ok(Json(MemoryUnitDto::from(unit)))
}
