use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct RawEvent {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub source: Source,
    pub event_type: EventType,
    pub actor: String,
    pub payload: serde_json::Value,
    pub idempotency_key: String,
    pub occurred_at: DateTime<Utc>,
    pub ingested_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[serde(rename_all = "lowercase")]
#[sqlx(type_name = "source", rename_all = "lowercase")]
pub enum Source {
    GitHub,
    Slack,
    Jira,
    Linear,
    Observation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[serde(rename_all = "snake_case")]
#[sqlx(type_name = "event_type", rename_all = "snake_case")]
pub enum EventType {
    PullRequest,
    PullRequestReview,
    Push,
    IssueComment,
    Issue,
    Message,
    Reaction,
    AgentObservation,
}
