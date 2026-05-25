use crate::{
    error::AppResult,
    models::WorkspaceConfig,
    workspace_config::{load_workspace_config, load_workspace_half_life_days},
};
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Clone)]
pub struct WorkspaceConfigService {
    db: PgPool,
}

impl WorkspaceConfigService {
    pub fn new(db: PgPool) -> Self {
        Self { db }
    }

    pub async fn load(&self, workspace_id: Uuid) -> AppResult<WorkspaceConfig> {
        load_workspace_config(&self.db, workspace_id).await
    }

    pub async fn half_life_days(&self, workspace_id: Uuid) -> AppResult<f64> {
        load_workspace_half_life_days(&self.db, workspace_id).await
    }
}