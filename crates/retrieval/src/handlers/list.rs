use axum::{extract::Query, extract::State, Json};
use common::{error::AppResult, AppError, AppState};
use validator::Validate;

use crate::{
    dto::{ListQuery, ListResponse, MemoryUnitDto},
    store,
};

#[axum::debug_handler]
pub async fn handle_list(
    State(state): State<AppState>,
    Query(params): Query<ListQuery>,
) -> AppResult<Json<ListResponse>> {
    Validate::validate(&params).map_err(|error| AppError::Validation(error.to_string()))?;

    let limit = params.resolved_limit();
    let offset = params.resolved_offset();
    let (items, total) = store::list_memory_units(&state.db, &params).await?;

    Ok(Json(ListResponse {
        items: items.into_iter().map(MemoryUnitDto::from).collect(),
        total,
        limit,
        offset,
    }))
}
