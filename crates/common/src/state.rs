use std::sync::Arc;

use qdrant_client::Qdrant;
use redis::aio::ConnectionManager;
use sqlx::PgPool;

use crate::{
    config::AppConfig,
    providers::{EmbeddingProvider, LlmProvider},
};

#[derive(Clone)]
pub struct AppState {
    pub db: PgPool,
    pub redis: ConnectionManager,
    pub qdrant: Qdrant,
    pub embedding_provider: Arc<dyn EmbeddingProvider>,
    pub llm_provider: Arc<dyn LlmProvider>,
    pub config: Arc<AppConfig>,
    pub github_webhook_secret: String,
}
