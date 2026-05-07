use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::raw_event::Source;

pub const DEFAULT_DECAY_HALF_LIFE_DAYS: u32 = 30;
pub const DEFAULT_PRUNING_THRESHOLD: f32 = 0.10;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Workspace {
    pub id: Uuid,
    pub name: String,
    pub config: serde_json::Value,
    pub promotion_threshold: f32,
    pub dedup_cosine_threshold: f32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub deleted_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WorkspaceConfig {
    #[serde(default = "default_promotion_threshold")]
    pub promotion_threshold: f32,
    #[serde(default = "default_dedup_cosine_threshold")]
    pub dedup_cosine_threshold: f32,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub llm_model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub embedding_model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub decay_half_life_days: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pruning_threshold: Option<f32>,
    #[serde(default = "default_contradiction_mode")]
    pub contradiction_mode: ContradictionMode,
    #[serde(default = "default_contradiction_threshold")]
    pub contradiction_threshold: f32,
    #[serde(default = "default_contradiction_candidates")]
    pub contradiction_candidates: usize,
    #[serde(default)]
    pub sub_agent_pools: Vec<String>,
    /// Maximum age in days before a memory is eligible for compliance hard-purge.
    /// None = no retention limit (default, no automatic purge).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retention_max_age_days: Option<u32>,
    /// When true, right-to-erasure and retention purges also hard-delete the
    /// originating raw_events. When false, only memory_units are affected.
    #[serde(default)]
    pub compliance_hard_purge: bool,
}

impl Default for WorkspaceConfig {
    fn default() -> Self {
        Self {
            promotion_threshold: default_promotion_threshold(),
            dedup_cosine_threshold: default_dedup_cosine_threshold(),
            access_count_trigger: default_access_count_trigger(),
            half_life_days: default_half_life_days(),
            decay_rate_episodic: default_decay_rate_episodic(),
            decay_rate_semantic: default_decay_rate_semantic(),
            llm_provider: None,
            embedding_provider: None,
            llm_model: None,
            embedding_model: None,
            decay_half_life_days: None,
            pruning_threshold: None,
            contradiction_mode: default_contradiction_mode(),
            contradiction_threshold: default_contradiction_threshold(),
            contradiction_candidates: default_contradiction_candidates(),
            sub_agent_pools: Vec::new(),
            retention_max_age_days: None,
            compliance_hard_purge: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ContradictionMode {
    #[default]
    Quarantine,
    AutoResolve,
}

fn default_promotion_threshold() -> f32 {
    0.72
}

fn default_dedup_cosine_threshold() -> f32 {
    0.92
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

fn default_contradiction_mode() -> ContradictionMode {
    ContradictionMode::Quarantine
}

fn default_contradiction_threshold() -> f32 {
    0.35
}

fn default_contradiction_candidates() -> usize {
    20
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

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn lifecycle_config_fields_round_trip_json() {
        let config = WorkspaceConfig {
            decay_half_life_days: Some(14),
            pruning_threshold: Some(0.05),
            llm_model: Some("llama3".to_owned()),
            embedding_model: Some("BAAI/bge-small-en-v1.5".to_owned()),
            ..WorkspaceConfig::default()
        };

        let value = match serde_json::to_value(&config) {
            Ok(value) => value,
            Err(error) => panic!("workspace config should serialize: {error}"),
        };

        assert_eq!(
            value
                .get("decay_half_life_days")
                .and_then(|value| value.as_u64()),
            Some(14)
        );
        assert!(value.get("pruning_threshold").is_some());

        let decoded = match serde_json::from_value::<WorkspaceConfig>(value) {
            Ok(decoded) => decoded,
            Err(error) => panic!("workspace config should deserialize: {error}"),
        };

        assert_eq!(decoded.decay_half_life_days, Some(14));
        assert_eq!(decoded.pruning_threshold, Some(0.05));
        assert_eq!(decoded.llm_model, Some("llama3".to_owned()));
        assert_eq!(
            decoded.embedding_model,
            Some("BAAI/bge-small-en-v1.5".to_owned())
        );
    }

    #[test]
    fn lifecycle_config_absent_fields_deserialize_to_none() {
        let decoded = match serde_json::from_value::<WorkspaceConfig>(json!({})) {
            Ok(decoded) => decoded,
            Err(error) => panic!("workspace config should deserialize: {error}"),
        };

        assert_eq!(decoded.decay_half_life_days, None);
        assert_eq!(decoded.pruning_threshold, None);
    }

    #[test]
    fn compliance_fields_round_trip_json() {
        let config = WorkspaceConfig {
            retention_max_age_days: Some(365),
            compliance_hard_purge: true,
            ..WorkspaceConfig::default()
        };
        let value = serde_json::to_value(&config).unwrap();
        assert_eq!(
            value
                .get("retention_max_age_days")
                .and_then(|value| value.as_u64()),
            Some(365)
        );
        assert_eq!(
            value
                .get("compliance_hard_purge")
                .and_then(|value| value.as_bool()),
            Some(true)
        );
        let decoded: WorkspaceConfig = serde_json::from_value(value).unwrap();
        assert_eq!(decoded.retention_max_age_days, Some(365));
        assert!(decoded.compliance_hard_purge);
    }

    #[test]
    fn compliance_fields_absent_deserialize_to_defaults() {
        let decoded: WorkspaceConfig = serde_json::from_value(serde_json::json!({})).unwrap();
        assert_eq!(decoded.retention_max_age_days, None);
        assert!(!decoded.compliance_hard_purge);
    }
}
