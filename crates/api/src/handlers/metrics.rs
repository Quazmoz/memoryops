use axum::{extract::Path, Extension};
use common::{auth::AuthContext, error::AppResult, AppError};
use uuid::Uuid;

use super::require_workspace;

#[axum::debug_handler]
pub async fn get_metrics(
    Extension(auth): Extension<AuthContext>,
    Path(id): Path<Uuid>,
) -> AppResult<()> {
    require_workspace(&auth, id)?;
    // Process-global metrics are not safe to expose per-workspace until
    // per-workspace partitioning is implemented.
    // See: https://github.com/Quazmoz/memoryops/issues/1
    Err(AppError::NotImplemented(
        "per-workspace metrics are not yet available; process-global values cannot be safely exposed per workspace"
            .to_owned(),
    ))
}
