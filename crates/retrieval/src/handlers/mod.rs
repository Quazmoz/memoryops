use std::collections::HashMap;

use common::{error::AppResult, AppError};
use uuid::Uuid;

pub mod get;
pub mod list;
pub mod search;
pub mod update;

pub(crate) fn workspace_id_param(params: &HashMap<String, String>) -> AppResult<Uuid> {
    let raw = params
        .get("workspace_id")
        .ok_or_else(|| AppError::Validation("missing workspace_id query parameter".to_owned()))?;

    Uuid::parse_str(raw)
        .map_err(|_| AppError::Validation("invalid workspace_id query parameter".to_owned()))
}
