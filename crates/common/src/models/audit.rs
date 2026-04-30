use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct AuditEntry {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub actor: String,
    pub action: AuditAction,
    pub target_id: Uuid,
    pub target_type: String,
    pub diff: Option<serde_json::Value>,
    pub occurred_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[serde(rename_all = "snake_case")]
#[sqlx(type_name = "audit_action", rename_all = "snake_case")]
pub enum AuditAction {
    MemoryCreated,
    MemoryEdited,
    MemoryDeleted,
    MemoryRestored,
    MemoryPinned,
    MemoryUnpinned,
    MemoryPromoted,
    MemoryMerged,
    MemoryEmbedded,
    MemoryHardDeleted,
    ImportanceOverridden,
    KeyCreated,
    KeyRevoked,
    ConfigUpdated,
    WorkspaceConfigUpdated,
    Publish,
    #[serde(rename = "workspace.promote")]
    #[sqlx(rename = "workspace.promote")]
    WorkspacePromote,
    IntegrationAdded,
    IntegrationRemoved,
    ContradictionResolved,
    WorkspaceReindexed,
    ObservationIngested,
    UserErasure,
}
