use std::collections::HashMap;

use axum::{
    extract::{Query, State},
    Extension, Json,
};
use common::{
    auth::AuthContext,
    error::AppResult,
    models::{MemoryType, ScopeVisibility},
    AppError, AppState,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use uuid::Uuid;

use super::{resolve_workspace_id, workspace_id_param};

#[derive(Debug, Deserialize)]
pub struct CreateMemoryRequest {
    pub content: String,
    pub workspace_id: Option<Uuid>,
    #[serde(default = "default_memory_type")]
    pub memory_type: MemoryType,
    #[serde(default = "default_importance")]
    pub importance_score: f32,
    #[serde(default)]
    pub tags: Vec<String>,
    pub agent_id: Option<String>,
    pub user_id: Option<String>,
    pub repo: Option<String>,
    pub scope_visibility: Option<ScopeVisibility>,
    // Accepted but not stored — kept for seed script compatibility
    pub metadata: Option<serde_json::Value>,
}

fn default_memory_type() -> MemoryType {
    MemoryType::Episodic
}

fn default_importance() -> f32 {
    0.5
}

#[derive(Debug, Serialize)]
pub struct CreateMemoryResponse {
    pub memory_id: Uuid,
    pub id: Uuid,
    pub scope_visibility: ScopeVisibility,
}

#[axum::debug_handler]
pub async fn handle_create(
    State(state): State<AppState>,
    auth: Option<Extension<AuthContext>>,
    Query(params): Query<HashMap<String, String>>,
    Json(req): Json<CreateMemoryRequest>,
) -> AppResult<Json<CreateMemoryResponse>> {
    if req.content.trim().is_empty() {
        return Err(AppError::Validation("content is required".to_owned()));
    }
    if !(0.0..=1.0).contains(&req.importance_score) {
        return Err(AppError::Validation(
            "importance_score must be between 0.0 and 1.0".to_owned(),
        ));
    }

    let auth_context = auth.as_ref().map(|extension| &extension.0);
    let supplied = req
        .workspace_id
        .or_else(|| workspace_id_param(&params).ok().flatten());
    let workspace_id = resolve_workspace_id(auth_context, supplied)?;

    let id = Uuid::now_v7();
    let agent_id = normalize_scope_value(req.agent_id);
    let user_id = normalize_scope_value(req.user_id);
    let repo = normalize_scope_value(req.repo);
    let scope_visibility = req
        .scope_visibility
        .unwrap_or_else(|| default_scope_visibility(agent_id.as_ref(), user_id.as_ref(), repo.as_ref()));
    let scope = json!({
        "workspace_id": workspace_id,
        "agent_id": agent_id,
        "user_id": user_id,
        "repo": repo,
    });

    sqlx::query_scalar::<_, Uuid>(
        r#"
        INSERT INTO memory_units (
            id, workspace_id, scope, memory_type, scope_visibility,
            content, entities, importance_score,
            source_events, embedding_id, token_count, tags
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, NULL, NULL, $10)
        RETURNING id
        "#,
    )
    .bind(id)
    .bind(workspace_id)
    .bind(scope)
    .bind(req.memory_type)
    .bind(scope_visibility)
    .bind(&req.content)
    .bind(json!([]))
    .bind(req.importance_score)
    .bind(Vec::<Uuid>::new())
    .bind(&req.tags)
    .fetch_one(&state.db)
    .await
    .map_err(AppError::Database)?;

    match state.redis.get().await {
        Ok(mut conn) => {
            if let Err(error) =
                processor::worker::enqueue_slow_job(&mut *conn, id, workspace_id, 0).await
            {
                tracing::warn!(error = ?error, memory_id = %id, "failed to enqueue memory for embedding");
            }
        }
        Err(error) => {
            tracing::warn!(error = ?error, memory_id = %id, "failed to get Redis connection for enqueue")
        }
    }

    Ok(Json(CreateMemoryResponse {
        memory_id: id,
        id,
        scope_visibility,
    }))
}

fn normalize_scope_value(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

fn default_scope_visibility(
    agent_id: Option<&String>,
    user_id: Option<&String>,
    repo: Option<&String>,
) -> ScopeVisibility {
    if agent_id.is_none() && user_id.is_none() && repo.is_none() {
        ScopeVisibility::Workspace
    } else {
        ScopeVisibility::Private
    }
}
