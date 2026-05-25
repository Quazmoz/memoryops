use common::{
    error::AppResult,
    models::MemoryUnit,
    services::VectorIndexService,
    AppError, AppState,
};
use uuid::Uuid;

use crate::store;

pub struct MemoryDeletionService<'a> {
    state: &'a AppState,
    collection_name: &'static str,
    log_context: &'static str,
}

impl<'a> MemoryDeletionService<'a> {
    pub fn new(
        state: &'a AppState,
        collection_name: &'static str,
        log_context: &'static str,
    ) -> Self {
        Self {
            state,
            collection_name,
            log_context,
        }
    }

    pub async fn soft_delete_optional(
        &self,
        memory_id: Uuid,
        workspace_id: Uuid,
    ) -> AppResult<Option<MemoryUnit>> {
        let deleted = store::soft_delete_memory_unit(&self.state.db, memory_id, workspace_id).await?;
        if deleted.is_some() {
            self.vector_index()
                .delete_point_best_effort(memory_id, self.log_context)
                .await;
        }
        Ok(deleted)
    }

    pub async fn soft_delete_required(
        &self,
        memory_id: Uuid,
        workspace_id: Uuid,
    ) -> AppResult<MemoryUnit> {
        self.soft_delete_optional(memory_id, workspace_id)
            .await?
            .ok_or_else(|| AppError::NotFound {
                resource: format!("memory:{memory_id}"),
            })
    }

    fn vector_index(&self) -> VectorIndexService<'_> {
        VectorIndexService::new(&self.state.qdrant, self.collection_name)
    }
}