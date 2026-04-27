use axum::{extract::Query, extract::State, Extension, Json};
use common::{auth::AuthContext, error::AppResult, AppError, AppState};
use validator::Validate;

use crate::{
    dto::{ListQuery, ListResponse, MemoryUnitDto},
    store,
};

use super::resolve_workspace_id;

#[axum::debug_handler]
pub async fn handle_list(
    State(state): State<AppState>,
    auth: Option<Extension<AuthContext>>,
    Query(params): Query<ListQuery>,
) -> AppResult<Json<ListResponse>> {
    Validate::validate(&params).map_err(|error| AppError::Validation(error.to_string()))?;
    let auth_context = auth.as_ref().map(|extension| &extension.0);
    let workspace_id = resolve_workspace_id(auth_context, params.workspace_id)?;

    let limit = params.resolved_limit();
    let offset = params.resolved_offset();
    let (items, total) = store::list_memory_units(&state.db, &params, workspace_id).await?;

    Ok(Json(ListResponse {
        items: items.into_iter().map(MemoryUnitDto::from).collect(),
        total,
        limit,
        offset,
    }))
}
