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
