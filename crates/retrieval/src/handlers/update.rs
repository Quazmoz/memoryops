use std::collections::HashMap;

use axum::{
    extract::{Path, Query, State},
    Json,
};
use common::{error::AppResult, AppError, AppState};
use uuid::Uuid;
use validator::Validate;

use crate::{
    dto::{MemoryUnitDto, UpdateMemoryRequest},
    store,
};

use super::workspace_id_param;

#[axum::debug_handler]
pub async fn handle_update(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Query(params): Query<HashMap<String, String>>,
    Json(req): Json<UpdateMemoryRequest>,
) -> AppResult<Json<MemoryUnitDto>> {
    Validate::validate(&req).map_err(|error| AppError::Validation(error.to_string()))?;
    let workspace_id = workspace_id_param(&params)?;
    let unit = store::update_memory_unit(&state.db, id, workspace_id, &req)
        .await?
        .ok_or_else(|| AppError::NotFound {
            resource: format!("memory:{id}"),
        })?;

    Ok(Json(MemoryUnitDto::from(unit)))
}
