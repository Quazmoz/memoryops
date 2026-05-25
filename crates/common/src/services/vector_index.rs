use anyhow::anyhow;
use qdrant_client::{qdrant::DeletePointsBuilder, Qdrant};
use uuid::Uuid;

use crate::{error::AppResult, AppError};

pub struct VectorIndexService<'a> {
    qdrant: &'a Qdrant,
    collection_name: &'static str,
}

impl<'a> VectorIndexService<'a> {
    pub fn new(qdrant: &'a Qdrant, collection_name: &'static str) -> Self {
        Self {
            qdrant,
            collection_name,
        }
    }

    pub async fn delete_point(&self, memory_id: Uuid) -> AppResult<()> {
        self.delete_points([memory_id]).await
    }

    pub async fn delete_points(&self, memory_ids: impl IntoIterator<Item = Uuid>) -> AppResult<()> {
        let points = memory_ids
            .into_iter()
            .map(|id| id.to_string())
            .collect::<Vec<_>>();
        if points.is_empty() {
            return Ok(());
        }

        self.qdrant
            .delete_points(
                DeletePointsBuilder::new(self.collection_name)
                    .points(points)
                    .wait(true),
            )
            .await
            .map(|_| ())
            .map_err(|error| AppError::Internal(anyhow!("failed to delete vector points: {error}")))
    }

    pub async fn delete_point_best_effort(&self, memory_id: Uuid, context: &'static str) {
        if let Err(error) = self.delete_point(memory_id).await {
            tracing::warn!(error = ?error, memory_id = %memory_id, context, "failed to delete vector point");
        }
    }
}