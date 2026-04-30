use axum::{extract::State, http::StatusCode, Extension, Json};
use common::{auth::AuthContext, AppError, AppState};
use serde::Deserialize;

use super::ingest::{ingest_observation, ObservationInput, ObservationOutput};

#[derive(Debug, Deserialize)]
pub struct ObservationRequest {
    pub content: String,
    pub agent_id: String,
    pub user_id: Option<String>,
    pub repo: Option<String>,
    pub tags: Option<Vec<String>>,
    pub importance: Option<f32>,
    pub source_ref: Option<String>,
    pub scope_id: Option<uuid::Uuid>,
}

#[axum::debug_handler]
pub async fn handle_ingest_observation(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthContext>,
    Json(body): Json<ObservationRequest>,
) -> Result<(StatusCode, Json<ObservationOutput>), AppError> {
    let input = ObservationInput {
        content: body.content,
        agent_id: body.agent_id,
        user_id: body.user_id,
        repo: body.repo,
        tags: body.tags,
        importance: body.importance,
        source_ref: body.source_ref,
        scope_id: body.scope_id,
    };

    let output = ingest_observation(&state, auth.workspace_id, input).await?;
    Ok((StatusCode::ACCEPTED, Json(output)))
}
