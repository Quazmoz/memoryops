use std::collections::HashMap;

use axum::{
    extract::{Path, Query, State},
    Extension, Json,
};
use chrono::{Duration, Utc};
use common::{
    audit::spawn_audit_log,
    auth::AuthContext,
    error::AppResult,
    models::{AuditAction, MemoryType, MemoryUnit, MemoryVersion},
    AppError, AppState,
};
use qdrant_client::{
    qdrant::{PointsIdsList, SetPayloadPointsBuilder},
    Payload,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use uuid::Uuid;

use processor::worker::enqueue_slow_job;

use crate::{
    dto::MemoryUnitDto,
    search::vector::COLLECTION_NAME,
    services::MemoryDeletionService,
    store::{self, BulkStoreAction},
};

use super::{audit_actor, resolve_workspace_id, workspace_id_param};

const MAX_BULK_MEMORY_IDS: usize = 100;

#[derive(Debug, Deserialize)]
pub struct BulkMemoryRequest {
    pub ids: Vec<Uuid>,
    pub action: BulkMemoryAction,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum BulkMemoryAction {
    Pin,
    Unpin,
    Delete,
}

#[derive(Debug, Serialize)]
pub struct BulkMemoryResponse {
    pub affected: usize,
    pub affected_ids: Vec<Uuid>,
    pub requested: usize,
    pub action: BulkMemoryAction,
}

#[derive(Debug, Deserialize)]
pub struct MergeMemoryRequest {
    pub source_id: Uuid,
    pub target_id: Uuid,
}

#[axum::debug_handler]
pub async fn handle_delete(
    State(state): State<AppState>,
    auth: Option<Extension<AuthContext>>,
    Path(id): Path<Uuid>,
    Query(params): Query<HashMap<String, String>>,
) -> AppResult<Json<MemoryUnitDto>> {
    let auth_context = auth.as_ref().map(|extension| &extension.0);
    let workspace_id = resolve_workspace_id(auth_context, workspace_id_param(&params)?)?;
    let before = store::get_memory_unit_by_id(&state.db, id, workspace_id)
        .await?
        .ok_or_else(|| AppError::NotFound {
            resource: format!("memory:{id}"),
        })?;
    let deleted = MemoryDeletionService::new(&state, COLLECTION_NAME, "retrieval memory delete")
        .soft_delete_required(id, workspace_id)
        .await?;

    spawn_audit_log(
        state.db.clone(),
        workspace_id,
        audit_actor(auth_context),
        AuditAction::MemoryDeleted,
        id,
        "memory",
        Some(json!({ "before": before, "after": deleted })),
    );

    Ok(Json(MemoryUnitDto::from(deleted)))
}

#[axum::debug_handler]
pub async fn handle_restore(
    State(state): State<AppState>,
    auth: Option<Extension<AuthContext>>,
    Path(id): Path<Uuid>,
    Query(params): Query<HashMap<String, String>>,
) -> AppResult<Json<MemoryUnitDto>> {
    let auth_context = auth.as_ref().map(|extension| &extension.0);
    let workspace_id = resolve_workspace_id(auth_context, workspace_id_param(&params)?)?;
    let before = store::get_memory_unit_by_id_including_deleted(&state.db, id, workspace_id)
        .await?
        .ok_or_else(|| AppError::NotFound {
            resource: format!("memory:{id}"),
        })?;
    let deleted_at = before
        .deleted_at
        .ok_or_else(|| AppError::Conflict("memory is not currently deleted".to_owned()))?;

    if deleted_at + Duration::days(30) <= Utc::now() {
        return Err(AppError::Conflict(
            "memory can only be restored within 30 days of deletion".to_owned(),
        ));
    }

    let restored = store::restore_memory_unit(&state.db, id, workspace_id)
        .await?
        .ok_or_else(|| AppError::NotFound {
            resource: format!("memory:{id}"),
        })?;

    match state.redis.get().await {
        Ok(mut conn) => {
            if let Err(error) =
                enqueue_slow_job(&mut *conn, restored.id, restored.workspace_id, 0).await
            {
                tracing::warn!(error = ?error, memory_id = %restored.id, "failed to enqueue restored memory for re-embedding");
            }
        }
        Err(error) => {
            tracing::warn!(error = ?error, memory_id = %restored.id, "failed to get Redis connection for restore enqueue")
        }
    }

    spawn_audit_log(
        state.db.clone(),
        workspace_id,
        audit_actor(auth_context),
        AuditAction::MemoryRestored,
        id,
        "memory",
        Some(json!({ "before": before, "after": restored })),
    );

    Ok(Json(MemoryUnitDto::from(restored)))
}

#[axum::debug_handler]
pub async fn handle_promote(
    State(state): State<AppState>,
    auth: Option<Extension<AuthContext>>,
    Path(id): Path<Uuid>,
    Query(params): Query<HashMap<String, String>>,
) -> AppResult<Json<MemoryUnitDto>> {
    let auth_context = auth.as_ref().map(|extension| &extension.0);
    let workspace_id = resolve_workspace_id(auth_context, workspace_id_param(&params)?)?;
    let before = store::get_memory_unit_by_id(&state.db, id, workspace_id)
        .await?
        .ok_or_else(|| AppError::NotFound {
            resource: format!("memory:{id}"),
        })?;
    let promoted = store::force_promote_to_semantic(&state.db, id, workspace_id)
        .await?
        .ok_or_else(|| AppError::NotFound {
            resource: format!("memory:{id}"),
        })?;

    spawn_audit_log(
        state.db.clone(),
        workspace_id,
        audit_actor(auth_context),
        AuditAction::MemoryPromoted,
        id,
        "memory",
        Some(json!({ "before": before, "after": promoted })),
    );

    Ok(Json(MemoryUnitDto::from(promoted)))
}

#[axum::debug_handler]
pub async fn handle_publish(
    State(state): State<AppState>,
    auth: Option<Extension<AuthContext>>,
    Path(id): Path<Uuid>,
    Query(params): Query<HashMap<String, String>>,
) -> AppResult<Json<MemoryUnitDto>> {
    let auth_context = auth.as_ref().map(|extension| &extension.0);
    let workspace_id = resolve_workspace_id(auth_context, workspace_id_param(&params)?)?;
    let before = store::get_memory_unit_by_id(&state.db, id, workspace_id)
        .await?
        .ok_or_else(|| AppError::NotFound {
            resource: format!("memory:{id}"),
        })?;

    if before.memory_type != MemoryType::Semantic {
        return Err(AppError::Unprocessable(
            "only semantic memories can be published".to_owned(),
        ));
    }

    let published = store::publish_memory_unit(&state.db, id, workspace_id)
        .await?
        .ok_or_else(|| AppError::NotFound {
            resource: format!("memory:{id}"),
        })?;

    match state.redis.get().await {
        Ok(mut conn) => {
            if let Err(error) =
                enqueue_slow_job(&mut *conn, published.id, published.workspace_id, 0).await
            {
                tracing::warn!(error = ?error, memory_id = %published.id, "failed to enqueue published memory for payload refresh");
            }
        }
        Err(error) => {
            tracing::warn!(error = ?error, memory_id = %published.id, "failed to get Redis connection for publish enqueue")
        }
    }
    refresh_published_qdrant_payload(&state, &published).await;

    spawn_audit_log(
        state.db.clone(),
        workspace_id,
        audit_actor(auth_context),
        AuditAction::Publish,
        id,
        "memory",
        Some(json!({
            "before": crate::dto::scope_visibility_as_str(before.scope_visibility),
            "after": crate::dto::scope_visibility_as_str(published.scope_visibility)
        })),
    );

    Ok(Json(MemoryUnitDto::from(published)))
}

async fn refresh_published_qdrant_payload(state: &AppState, memory: &MemoryUnit) {
    if memory.embedding_id.is_none() {
        return;
    }

    let payload = match Payload::try_from(json!({
        "scope_visibility": crate::dto::scope_visibility_as_str(memory.scope_visibility)
    })) {
        Ok(payload) => payload,
        Err(error) => {
            tracing::warn!(error = ?error, memory_id = %memory.id, "failed to build Qdrant publish payload");
            return;
        }
    };

    let request = SetPayloadPointsBuilder::new(COLLECTION_NAME, payload)
        .points_selector(PointsIdsList {
            ids: vec![memory.id.to_string().into()],
        })
        .wait(true);

    if let Err(error) = state.qdrant.set_payload(request).await {
        tracing::warn!(error = ?error, memory_id = %memory.id, "failed to refresh published memory payload in Qdrant");
    }
}

#[axum::debug_handler]
pub async fn handle_bulk(
    State(state): State<AppState>,
    auth: Option<Extension<AuthContext>>,
    Query(params): Query<HashMap<String, String>>,
    Json(request): Json<BulkMemoryRequest>,
) -> AppResult<Json<BulkMemoryResponse>> {
    if request.ids.is_empty() {
        return Err(AppError::Validation("ids must not be empty".to_owned()));
    }
    if request.ids.len() > MAX_BULK_MEMORY_IDS {
        return Err(AppError::Validation(
            "bulk requests are limited to 100 ids".to_owned(),
        ));
    }

    let auth_context = auth.as_ref().map(|extension| &extension.0);
    let workspace_id = resolve_workspace_id(auth_context, workspace_id_param(&params)?)?;
    let ids = unique_ids(request.ids);
    let requested = ids.len();

    let units = match request.action {
        BulkMemoryAction::Pin | BulkMemoryAction::Unpin => {
            let store_action = match request.action {
                BulkMemoryAction::Pin => BulkStoreAction::Pin,
                BulkMemoryAction::Unpin => BulkStoreAction::Unpin,
                _ => unreachable!(),
            };
            store::bulk_update_memory_units(&state.db, &ids, workspace_id, store_action).await?
        }
        BulkMemoryAction::Delete => {
            let deletion_service = MemoryDeletionService::new(&state, COLLECTION_NAME, "retrieval memory bulk delete");
            deletion_service.soft_delete_many_required(&ids, workspace_id).await?
        }
    };

    let audit_action = match request.action {
        BulkMemoryAction::Pin => AuditAction::MemoryPinned,
        BulkMemoryAction::Unpin => AuditAction::MemoryUnpinned,
        BulkMemoryAction::Delete => AuditAction::MemoryDeleted,
    };
    let actor = audit_actor(auth_context);
    for unit in &units {
        spawn_audit_log(
            state.db.clone(),
            workspace_id,
            actor.clone(),
            audit_action,
            unit.id,
            "memory",
            Some(json!({ "after": unit })),
        );
    }

    let affected_ids: Vec<Uuid> = units.iter().map(|unit| unit.id).collect();

    Ok(Json(BulkMemoryResponse {
        affected: units.len(),
        affected_ids,
        requested,
        action: request.action,
    }))
}

#[axum::debug_handler]
pub async fn handle_history(
    State(state): State<AppState>,
    auth: Option<Extension<AuthContext>>,
    Path(id): Path<Uuid>,
    Query(params): Query<HashMap<String, String>>,
) -> AppResult<Json<Vec<MemoryVersion>>> {
    let auth_context = auth.as_ref().map(|extension| &extension.0);
    let workspace_id = resolve_workspace_id(auth_context, workspace_id_param(&params)?)?;
    let unit = store::get_memory_unit_by_id(&state.db, id, workspace_id)
        .await?
        .ok_or_else(|| AppError::NotFound {
            resource: format!("memory:{id}"),
        })?;

    if unit.memory_type != MemoryType::Semantic {
        return Ok(Json(Vec::new()));
    }

    Ok(Json(
        store::list_memory_versions(&state.db, id, workspace_id).await?,
    ))
}

#[axum::debug_handler]
pub async fn handle_merge(
    State(state): State<AppState>,
    auth: Option<Extension<AuthContext>>,
    Query(params): Query<HashMap<String, String>>,
    Json(request): Json<MergeMemoryRequest>,
) -> AppResult<Json<MemoryUnitDto>> {
    let auth_context = auth.as_ref().map(|extension| &extension.0);
    let workspace_id = resolve_workspace_id(auth_context, workspace_id_param(&params)?)?;
    let actor = audit_actor(auth_context);
    let result = store::merge_memory_units(
        &state.db,
        request.source_id,
        request.target_id,
        workspace_id,
        &actor,
    )
    .await?;

    spawn_audit_log(
        state.db.clone(),
        workspace_id,
        actor,
        AuditAction::MemoryMerged,
        request.target_id,
        "memory",
        Some(json!({
            "source": result.source,
            "target_before": result.target_before,
            "target_after": result.target_after
        })),
    );

    Ok(Json(MemoryUnitDto::from(result.target_after)))
}

fn unique_ids(ids: Vec<Uuid>) -> Vec<Uuid> {
    let mut unique = Vec::with_capacity(ids.len());
    for memory_id in ids {
        if !unique.contains(&memory_id) {
            unique.push(memory_id);
        }
    }
    unique
}
