use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{
    error::BoxDynError,
    postgres::{PgTypeInfo, PgValueRef},
    types::Json,
    Decode, Postgres, Type,
};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct MemoryUnit {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub scope: MemoryScope,
    pub memory_type: MemoryType,
    pub content: String,
    pub entities: Json<Vec<Entity>>,
    pub importance_score: f32,
    pub importance_overridden: bool,
    pub source_events: Vec<Uuid>,
    pub embedding_id: Option<String>,
    pub token_count: Option<i32>,
    pub decay_score: f32,
    pub pinned: bool,
    pub tags: Vec<String>,
    pub version: i32,
    pub promoted_at: Option<DateTime<Utc>>,
    pub source_episode_ids: Vec<Uuid>,
    pub corroboration_count: i32,
    pub deleted_at: Option<DateTime<Utc>>,
    pub last_accessed_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl MemoryUnit {
    pub fn is_semantic(&self) -> bool {
        self.memory_type == MemoryType::Semantic
    }

    pub fn is_episodic(&self) -> bool {
        self.memory_type == MemoryType::Episodic
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[serde(rename_all = "lowercase")]
#[sqlx(type_name = "memory_type", rename_all = "lowercase")]
pub enum MemoryType {
    Episodic,
    Semantic,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryScope {
    pub workspace_id: Uuid,
    pub agent_id: Option<String>,
    pub user_id: Option<String>,
    pub repo: Option<String>,
}

impl MemoryScope {
    pub fn specificity(&self) -> u8 {
        let mut score = 0u8;
        if self.agent_id.is_some() {
            score += 4;
        }
        if self.user_id.is_some() {
            score += 2;
        }
        if self.repo.is_some() {
            score += 1;
        }
        score
    }
}

impl Type<Postgres> for MemoryScope {
    fn type_info() -> PgTypeInfo {
        <Json<MemoryScope> as Type<Postgres>>::type_info()
    }

    fn compatible(ty: &PgTypeInfo) -> bool {
        <Json<MemoryScope> as Type<Postgres>>::compatible(ty)
    }
}

impl<'r> Decode<'r, Postgres> for MemoryScope {
    fn decode(value: PgValueRef<'r>) -> Result<Self, BoxDynError> {
        let decoded = <Json<MemoryScope> as Decode<Postgres>>::decode(value)?;
        Ok(decoded.0)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Entity {
    pub entity_type: EntityType,
    pub value: String,
    pub confidence: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EntityType {
    Person,
    Repo,
    Branch,
    Topic,
    File,
    Team,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct MemoryVersion {
    pub id: Uuid,
    pub memory_id: Uuid,
    pub workspace_id: Uuid,
    pub version: i32,
    pub content: String,
    pub importance_score: f32,
    pub tags: Vec<String>,
    pub edited_by: String,
    pub created_at: DateTime<Utc>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_scope_specificity_uses_agent_user_repo_weights() {
        let scope = MemoryScope {
            workspace_id: Uuid::now_v7(),
            agent_id: Some("agent".to_owned()),
            user_id: Some("user".to_owned()),
            repo: Some("Quazmoz/memoryops".to_owned()),
        };

        assert_eq!(scope.specificity(), 7);
    }
}
