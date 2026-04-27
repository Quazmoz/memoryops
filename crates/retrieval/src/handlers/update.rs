use std::collections::HashMap;

use axum::{
    extract::{Path, Query, State},
    Extension, Json,
};
use common::{
    audit::spawn_audit_log, auth::AuthContext, error::AppResult, models::AuditAction, AppError,
    AppState,
};
use serde_json::json;
use uuid::Uuid;
use validator::Validate;

use crate::{
    dto::{MemoryUnitDto, UpdateMemoryRequest},
    store,
};

use super::{audit_actor, resolve_workspace_id, workspace_id_param};

#[axum::debug_handler]
pub async fn handle_update(
    State(state): State<AppState>,
    auth: Option<Extension<AuthContext>>,
    Path(id): Path<Uuid>,
    Query(params): Query<HashMap<String, String>>,
    Json(req): Json<UpdateMemoryRequest>,
) -> AppResult<Json<MemoryUnitDto>> {
    Validate::validate(&req).map_err(|error| AppError::Validation(error.to_string()))?;
    let auth_context = auth.as_ref().map(|extension| &extension.0);
    let workspace_id = resolve_workspace_id(auth_context, workspace_id_param(&params)?)?;
    let before = if req.is_empty() {
        None
    } else {
        store::get_memory_unit_by_id(&state.db, id, workspace_id).await?
    };
    let unit = store::update_memory_unit(&state.db, id, workspace_id, &req)
        .await?
        .ok_or_else(|| AppError::NotFound {
            resource: format!("memory:{id}"),
        })?;

    if let Some(before) = before {
        spawn_audit_log(
            state.db.clone(),
            workspace_id,
            audit_actor(auth_context),
            audit_action(&req),
            id,
            "memory",
            Some(json!({ "before": before, "after": unit })),
        );
    }

    Ok(Json(MemoryUnitDto::from(unit)))
}

fn audit_action(request: &UpdateMemoryRequest) -> AuditAction {
    if request.importance_score.is_some() {
        AuditAction::ImportanceOverridden
    } else if let Some(pinned) = request.pinned {
        if pinned {
            AuditAction::MemoryPinned
        } else {
            AuditAction::MemoryUnpinned
        }
    } else {
        AuditAction::MemoryEdited
    }
}
