use axum::{extract::Path, response::IntoResponse, Extension, Json};
use chrono::{DateTime, Utc};
use common::{
    auth::AuthContext,
    error::AppResult,
    telemetry::{metrics_snapshot, MetricsValues},
};
use serde::Serialize;
use uuid::Uuid;

use super::require_workspace;

// NOTE: counter and histogram instruments are global to the API process — they
// are not partitioned per workspace. The endpoint accepts a workspace ID so the
// API key middleware can enforce workspace ownership of the caller, but the
// values returned here describe the entire process.
#[derive(Debug, Serialize)]
pub struct MetricsResponse {
    pub workspace_id: Uuid,
    pub collected_at: DateTime<Utc>,
    // SECURITY: values are process-global and not partitioned by workspace.
    // TODO(security): track per-workspace metrics partitioning as a follow-up.
    pub metrics: MetricsValues,
}

#[axum::debug_handler]
pub async fn get_metrics(
    Extension(auth): Extension<AuthContext>,
    Path(id): Path<Uuid>,
) -> AppResult<impl IntoResponse> {
    require_workspace(&auth, id)?;
    Ok(Json(MetricsResponse {
        workspace_id: id,
        collected_at: Utc::now(),
        metrics: metrics_snapshot(),
    }))
}
