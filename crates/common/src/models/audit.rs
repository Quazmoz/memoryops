use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A persisted audit log row.
///
/// The original columns (`id`, `workspace_id`, `actor`, `action`, `target_id`,
/// `target_type`, `diff`, `occurred_at`) are always present. Everything added by
/// the production audit hardening work is optional so that rows written before
/// the upgrade still deserialize cleanly, and so API responses stay compact.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct AuditEntry {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub actor: String,
    pub action: AuditAction,
    pub target_id: Uuid,
    pub target_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub diff: Option<serde_json::Value>,
    pub occurred_at: DateTime<Utc>,

    // ── Request / actor / target context ─────────────────────────────────────
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub request_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub correlation_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub actor_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub actor_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub actor_display: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_key_id: Option<Uuid>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_key_prefix: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_ip: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user_agent: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub route: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub method: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status_code: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_version: Option<i32>,

    // ── Classification / outcome ─────────────────────────────────────────────
    #[serde(default = "default_severity")]
    pub severity: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    #[serde(default = "default_success")]
    pub success: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_code: Option<String>,

    // ── Structured (redacted) payloads ───────────────────────────────────────
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub before: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub after: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Value>,

    // ── Tamper-evidence ──────────────────────────────────────────────────────
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seq: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prev_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hash: Option<String>,
}

fn default_severity() -> String {
    "info".to_owned()
}

fn default_success() -> bool {
    true
}

/// Comma/space tolerant column list for `SELECT`ing a full [`AuditEntry`].
///
/// Centralised so the list handler, single-entry handler, and export handler all
/// stay in sync with the struct above.
pub const AUDIT_ENTRY_COLUMNS: &str = "id, workspace_id, actor, action, target_id, target_type, \
diff, occurred_at, request_id, correlation_id, actor_type, actor_id, actor_display, api_key_id, \
api_key_prefix, source_ip, user_agent, route, method, status_code, reason, target_name, \
target_version, severity, category, success, error_code, before, after, metadata, seq, prev_hash, \
hash";

/// Severity levels for audit events, ordered least → most serious.
pub const AUDIT_SEVERITIES: [&str; 4] = ["info", "notice", "warning", "critical"];

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
    MemoryImported,
    MemoryExported,
    ImportanceOverridden,
    KeyCreated,
    KeyRevoked,
    ConfigUpdated,
    WorkspaceConfigUpdated,
    Publish,
    #[serde(rename = "workspace.promote")]
    #[sqlx(rename = "workspace.promote")]
    WorkspacePromote,
    WorkspaceCreated,
    WorkspaceBootstrap,
    IntegrationAdded,
    IntegrationUpdated,
    IntegrationRemoved,
    IntegrationWebhookSecretChanged,
    ContradictionResolved,
    ContradictionDismissed,
    RetrievalFeedback,
    WorkspaceReindexed,
    WorkspaceDeleted,
    ObservationIngested,
    UserErasure,
    ToolCreated,
    ToolUpdated,
    ToolDeleted,
    ToolRolledBack,
    ToolInvoked,
    ToolSecretRevealed,
    AgentResourceCreated,
    AgentResourceUpdated,
    AgentResourceDeleted,
    AgentResourceRolledBack,
    AuthFailed,
    AuditExported,
}

/// Coarse functional grouping used for investigation filters and the
/// `/audit/actions` discovery endpoint.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuditCategory {
    Memory,
    Workspace,
    ApiKey,
    Integration,
    Tool,
    AgentResource,
    Retrieval,
    Contradiction,
    Compliance,
    Security,
    System,
}

impl AuditCategory {
    pub fn as_str(self) -> &'static str {
        match self {
            AuditCategory::Memory => "memory",
            AuditCategory::Workspace => "workspace",
            AuditCategory::ApiKey => "api_key",
            AuditCategory::Integration => "integration",
            AuditCategory::Tool => "tool",
            AuditCategory::AgentResource => "agent_resource",
            AuditCategory::Retrieval => "retrieval",
            AuditCategory::Contradiction => "contradiction",
            AuditCategory::Compliance => "compliance",
            AuditCategory::Security => "security",
            AuditCategory::System => "system",
        }
    }
}

/// Severity classification for an action.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuditSeverity {
    Info,
    Notice,
    Warning,
    Critical,
}

impl AuditSeverity {
    pub fn as_str(self) -> &'static str {
        match self {
            AuditSeverity::Info => "info",
            AuditSeverity::Notice => "notice",
            AuditSeverity::Warning => "warning",
            AuditSeverity::Critical => "critical",
        }
    }
}

/// Static metadata describing one audit action, surfaced through the
/// `/audit/actions` endpoint and used to classify rows at write time.
#[derive(Debug, Clone, Serialize)]
pub struct AuditActionInfo {
    /// Canonical wire name (matches the SQL enum + serde representation).
    pub name: &'static str,
    pub category: &'static str,
    pub default_severity: &'static str,
    /// Whether this action is treated as security/compliance-critical and must
    /// use the reliable (synchronous, error-propagating) write path rather than
    /// best-effort.
    pub required: bool,
}

impl AuditAction {
    /// Canonical wire name (matches the Postgres enum and serde rename rules).
    pub fn as_str(self) -> &'static str {
        match self {
            AuditAction::MemoryCreated => "memory_created",
            AuditAction::MemoryEdited => "memory_edited",
            AuditAction::MemoryDeleted => "memory_deleted",
            AuditAction::MemoryRestored => "memory_restored",
            AuditAction::MemoryPinned => "memory_pinned",
            AuditAction::MemoryUnpinned => "memory_unpinned",
            AuditAction::MemoryPromoted => "memory_promoted",
            AuditAction::MemoryMerged => "memory_merged",
            AuditAction::MemoryEmbedded => "memory_embedded",
            AuditAction::MemoryHardDeleted => "memory_hard_deleted",
            AuditAction::MemoryImported => "memory_imported",
            AuditAction::MemoryExported => "memory_exported",
            AuditAction::ImportanceOverridden => "importance_overridden",
            AuditAction::KeyCreated => "key_created",
            AuditAction::KeyRevoked => "key_revoked",
            AuditAction::ConfigUpdated => "config_updated",
            AuditAction::WorkspaceConfigUpdated => "workspace_config_updated",
            AuditAction::Publish => "publish",
            AuditAction::WorkspacePromote => "workspace.promote",
            AuditAction::WorkspaceCreated => "workspace_created",
            AuditAction::WorkspaceBootstrap => "workspace_bootstrap",
            AuditAction::IntegrationAdded => "integration_added",
            AuditAction::IntegrationUpdated => "integration_updated",
            AuditAction::IntegrationRemoved => "integration_removed",
            AuditAction::IntegrationWebhookSecretChanged => "integration_webhook_secret_changed",
            AuditAction::ContradictionResolved => "contradiction_resolved",
            AuditAction::ContradictionDismissed => "contradiction_dismissed",
            AuditAction::RetrievalFeedback => "retrieval_feedback",
            AuditAction::WorkspaceReindexed => "workspace_reindexed",
            AuditAction::WorkspaceDeleted => "workspace_deleted",
            AuditAction::ObservationIngested => "observation_ingested",
            AuditAction::UserErasure => "user_erasure",
            AuditAction::ToolCreated => "tool_created",
            AuditAction::ToolUpdated => "tool_updated",
            AuditAction::ToolDeleted => "tool_deleted",
            AuditAction::ToolRolledBack => "tool_rolled_back",
            AuditAction::ToolInvoked => "tool_invoked",
            AuditAction::ToolSecretRevealed => "tool_secret_revealed",
            AuditAction::AgentResourceCreated => "agent_resource_created",
            AuditAction::AgentResourceUpdated => "agent_resource_updated",
            AuditAction::AgentResourceDeleted => "agent_resource_deleted",
            AuditAction::AgentResourceRolledBack => "agent_resource_rolled_back",
            AuditAction::AuthFailed => "auth_failed",
            AuditAction::AuditExported => "audit_exported",
        }
    }

    pub fn category(self) -> AuditCategory {
        match self {
            AuditAction::MemoryCreated
            | AuditAction::MemoryEdited
            | AuditAction::MemoryDeleted
            | AuditAction::MemoryRestored
            | AuditAction::MemoryPinned
            | AuditAction::MemoryUnpinned
            | AuditAction::MemoryPromoted
            | AuditAction::MemoryMerged
            | AuditAction::MemoryEmbedded
            | AuditAction::MemoryHardDeleted
            | AuditAction::MemoryImported
            | AuditAction::MemoryExported
            | AuditAction::ImportanceOverridden
            | AuditAction::Publish
            | AuditAction::ObservationIngested => AuditCategory::Memory,
            AuditAction::KeyCreated | AuditAction::KeyRevoked => AuditCategory::ApiKey,
            AuditAction::ConfigUpdated
            | AuditAction::WorkspaceConfigUpdated
            | AuditAction::WorkspaceCreated
            | AuditAction::WorkspaceDeleted
            | AuditAction::WorkspaceReindexed
            | AuditAction::WorkspacePromote => AuditCategory::Workspace,
            AuditAction::WorkspaceBootstrap
            | AuditAction::AuthFailed
            | AuditAction::AuditExported => AuditCategory::Security,
            AuditAction::IntegrationAdded
            | AuditAction::IntegrationUpdated
            | AuditAction::IntegrationRemoved
            | AuditAction::IntegrationWebhookSecretChanged => AuditCategory::Integration,
            AuditAction::ContradictionResolved | AuditAction::ContradictionDismissed => {
                AuditCategory::Contradiction
            }
            AuditAction::RetrievalFeedback => AuditCategory::Retrieval,
            AuditAction::UserErasure => AuditCategory::Compliance,
            AuditAction::ToolCreated
            | AuditAction::ToolUpdated
            | AuditAction::ToolDeleted
            | AuditAction::ToolRolledBack
            | AuditAction::ToolInvoked
            | AuditAction::ToolSecretRevealed => AuditCategory::Tool,
            AuditAction::AgentResourceCreated
            | AuditAction::AgentResourceUpdated
            | AuditAction::AgentResourceDeleted
            | AuditAction::AgentResourceRolledBack => AuditCategory::AgentResource,
        }
    }

    pub fn default_severity(self) -> AuditSeverity {
        match self {
            // Security/compliance-critical: erasure, secret reveal, webhook secret
            // rotation, key lifecycle, workspace deletion, failed auth.
            AuditAction::UserErasure
            | AuditAction::ToolSecretRevealed
            | AuditAction::IntegrationWebhookSecretChanged
            | AuditAction::KeyCreated
            | AuditAction::KeyRevoked
            | AuditAction::WorkspaceDeleted
            | AuditAction::MemoryHardDeleted => AuditSeverity::Critical,
            AuditAction::AuthFailed => AuditSeverity::Warning,
            // Configuration & governance changes worth highlighting.
            AuditAction::ConfigUpdated
            | AuditAction::WorkspaceConfigUpdated
            | AuditAction::WorkspaceCreated
            | AuditAction::IntegrationAdded
            | AuditAction::IntegrationUpdated
            | AuditAction::IntegrationRemoved
            | AuditAction::ToolCreated
            | AuditAction::ToolUpdated
            | AuditAction::ToolDeleted
            | AuditAction::ToolRolledBack
            | AuditAction::AgentResourceCreated
            | AuditAction::AgentResourceUpdated
            | AuditAction::AgentResourceDeleted
            | AuditAction::AgentResourceRolledBack
            | AuditAction::AuditExported
            | AuditAction::MemoryImported
            | AuditAction::MemoryExported => AuditSeverity::Notice,
            _ => AuditSeverity::Info,
        }
    }

    /// Security/compliance-critical actions that must use the reliable
    /// (synchronous, error-propagating) write path. Best-effort writes are only
    /// acceptable for high-volume operational events.
    pub fn is_required(self) -> bool {
        matches!(
            self,
            AuditAction::KeyCreated
                | AuditAction::KeyRevoked
                | AuditAction::WorkspaceCreated
                | AuditAction::WorkspaceDeleted
                | AuditAction::WorkspaceConfigUpdated
                | AuditAction::ConfigUpdated
                | AuditAction::IntegrationAdded
                | AuditAction::IntegrationUpdated
                | AuditAction::IntegrationRemoved
                | AuditAction::IntegrationWebhookSecretChanged
                | AuditAction::ToolCreated
                | AuditAction::ToolUpdated
                | AuditAction::ToolDeleted
                | AuditAction::ToolRolledBack
                | AuditAction::ToolSecretRevealed
                | AuditAction::AgentResourceCreated
                | AuditAction::AgentResourceUpdated
                | AuditAction::AgentResourceDeleted
                | AuditAction::AgentResourceRolledBack
                | AuditAction::UserErasure
                | AuditAction::MemoryHardDeleted
                | AuditAction::AuditExported
        )
    }

    pub fn info(self) -> AuditActionInfo {
        AuditActionInfo {
            name: self.as_str(),
            category: self.category().as_str(),
            default_severity: self.default_severity().as_str(),
            required: self.is_required(),
        }
    }

    /// All known actions, for the discovery endpoint.
    pub fn all() -> &'static [AuditAction] {
        use AuditAction::*;
        &[
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
            MemoryImported,
            MemoryExported,
            ImportanceOverridden,
            KeyCreated,
            KeyRevoked,
            ConfigUpdated,
            WorkspaceConfigUpdated,
            Publish,
            WorkspacePromote,
            WorkspaceCreated,
            WorkspaceBootstrap,
            IntegrationAdded,
            IntegrationUpdated,
            IntegrationRemoved,
            IntegrationWebhookSecretChanged,
            ContradictionResolved,
            ContradictionDismissed,
            RetrievalFeedback,
            WorkspaceReindexed,
            WorkspaceDeleted,
            ObservationIngested,
            UserErasure,
            ToolCreated,
            ToolUpdated,
            ToolDeleted,
            ToolRolledBack,
            ToolInvoked,
            ToolSecretRevealed,
            AgentResourceCreated,
            AgentResourceUpdated,
            AgentResourceDeleted,
            AgentResourceRolledBack,
            AuthFailed,
            AuditExported,
        ]
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]
    use super::*;

    #[test]
    fn as_str_round_trips_through_serde() {
        for action in AuditAction::all() {
            let json = serde_json::to_string(action).expect("serialize");
            let expected = format!("\"{}\"", action.as_str());
            assert_eq!(json, expected, "as_str mismatch for {action:?}");
        }
    }

    #[test]
    fn critical_actions_are_required() {
        assert!(AuditAction::UserErasure.is_required());
        assert!(AuditAction::ToolSecretRevealed.is_required());
        assert!(AuditAction::KeyRevoked.is_required());
        // High-volume operational events stay best-effort.
        assert!(!AuditAction::MemoryEmbedded.is_required());
        assert!(!AuditAction::ObservationIngested.is_required());
        assert!(!AuditAction::ToolInvoked.is_required());
    }
}
