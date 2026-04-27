use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::raw_event::Source;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Workspace {
    pub id: Uuid,
    pub name: String,
    pub config: serde_json::Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub deleted_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WorkspaceConfig {
    #[serde(default = "default_promotion_threshold")]
    pub promotion_threshold: f32,
    #[serde(default = "default_access_count_trigger")]
    pub access_count_trigger: u32,
    #[serde(default = "default_half_life_days")]
    pub half_life_days: f32,
    #[serde(default = "default_decay_rate_episodic")]
    pub decay_rate_episodic: f32,
    #[serde(default = "default_decay_rate_semantic")]
    pub decay_rate_semantic: f32,
    pub llm_provider: Option<String>,
    pub embedding_provider: Option<String>,
}

impl Default for WorkspaceConfig {
    fn default() -> Self {
        Self {
            promotion_threshold: default_promotion_threshold(),
            access_count_trigger: default_access_count_trigger(),
            half_life_days: default_half_life_days(),
            decay_rate_episodic: default_decay_rate_episodic(),
            decay_rate_semantic: default_decay_rate_semantic(),
            llm_provider: None,
            embedding_provider: None,
        }
    }
}

fn default_promotion_threshold() -> f32 {
    0.85
}

fn default_access_count_trigger() -> u32 {
    3
}

fn default_half_life_days() -> f32 {
    30.0
}

fn default_decay_rate_episodic() -> f32 {
    1.0
}

fn default_decay_rate_semantic() -> f32 {
    0.5
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct ApiKey {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub name: String,
    pub key_hash: String,
    pub prefix: String,
    pub created_at: DateTime<Utc>,
    pub last_used_at: Option<DateTime<Utc>>,
    pub revoked: bool,
    pub revoked_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct IntegrationHealth {
    pub workspace_id: Uuid,
    pub source: Source,
    pub last_event_at: Option<DateTime<Utc>>,
    pub events_24h: i64,
    pub errors_24h: i64,
    pub status: IntegrationStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[serde(rename_all = "lowercase")]
#[sqlx(type_name = "integration_status", rename_all = "lowercase")]
pub enum IntegrationStatus {
    Active,
    Degraded,
    Failing,
}
