use std::collections::HashMap;

use common::{auth::AuthContext, error::AppResult, AppError};
use uuid::Uuid;

pub mod get;
pub mod list;
pub mod search;
pub mod update;

pub mod lifecycle;
pub mod retrieve;

pub(crate) fn workspace_id_param(params: &HashMap<String, String>) -> AppResult<Option<Uuid>> {
    let Some(raw) = params.get("workspace_id") else {
        return Ok(None);
    };

    Uuid::parse_str(raw)
        .map(Some)
        .map_err(|_| AppError::Validation("invalid workspace_id query parameter".to_owned()))
}

pub(crate) fn resolve_workspace_id(
    auth: Option<&AuthContext>,
    supplied_workspace_id: Option<Uuid>,
) -> AppResult<Uuid> {
    match (auth, supplied_workspace_id) {
        (Some(context), Some(workspace_id)) if context.workspace_id != workspace_id => {
            Err(AppError::Forbidden)
        }
        (Some(context), _) => Ok(context.workspace_id),
        (None, Some(workspace_id)) => Ok(workspace_id),
        (None, None) => Err(AppError::Validation(
            "missing workspace_id query parameter".to_owned(),
        )),
    }
}

pub(crate) fn audit_actor(auth: Option<&AuthContext>) -> String {
    auth.map(AuthContext::actor)
        .unwrap_or_else(|| "anonymous".to_owned())
}
