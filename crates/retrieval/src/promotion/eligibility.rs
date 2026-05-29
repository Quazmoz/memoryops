use common::models::{MemoryType, MemoryUnit, WorkspaceConfig};

pub fn is_eligible_for_promotion(
    unit: &MemoryUnit,
    access_count: u64,
    config: &WorkspaceConfig,
) -> bool {
    unit.memory_type == MemoryType::Episodic
        && unit.importance_score >= config.promotion_threshold
        && access_count >= u64::from(config.access_count_trigger)
        && unit.deleted_at.is_none()
        && !unit.pinned
}

#[cfg(test)]
mod tests {
    use chrono::Utc;
    use common::models::{Entity, MemoryScope, ScopeVisibility};
    use sqlx::types::Json;
    use uuid::Uuid;

    use super::*;

    #[test]
    fn episodic_high_importance_high_access_is_eligible() {
        let config = test_config();
        let unit = memory_unit(MemoryType::Episodic, 0.9, 3, false, false);

        assert!(is_eligible_for_promotion(&unit, 3, &config));
    }

    #[test]
    fn semantic_is_not_eligible() {
        let config = test_config();
        let unit = memory_unit(MemoryType::Semantic, 0.9, 3, false, false);

        assert!(!is_eligible_for_promotion(&unit, 3, &config));
    }

    #[test]
    fn low_importance_is_not_eligible() {
        let config = test_config();
        let unit = memory_unit(MemoryType::Episodic, 0.4, 3, false, false);

        assert!(!is_eligible_for_promotion(&unit, 3, &config));
    }

    #[test]
    fn low_access_count_is_not_eligible() {
        let config = test_config();
        let unit = memory_unit(MemoryType::Episodic, 0.9, 2, false, false);

        assert!(!is_eligible_for_promotion(&unit, 2, &config));
    }

    #[test]
    fn pinned_is_not_eligible() {
        let config = test_config();
        let unit = memory_unit(MemoryType::Episodic, 0.9, 3, true, false);

        assert!(!is_eligible_for_promotion(&unit, 3, &config));
    }

    #[test]
    fn deleted_is_not_eligible() {
        let config = test_config();
        let unit = memory_unit(MemoryType::Episodic, 0.9, 3, false, true);

        assert!(!is_eligible_for_promotion(&unit, 3, &config));
    }

    fn test_config() -> WorkspaceConfig {
        WorkspaceConfig::default()
    }

    fn memory_unit(
        memory_type: MemoryType,
        importance_score: f32,
        access_count: i32,
        pinned: bool,
        deleted: bool,
    ) -> MemoryUnit {
        let now = Utc::now();
        let workspace_id = Uuid::now_v7();
        MemoryUnit {
            id: Uuid::now_v7(),
            workspace_id,
            scope: MemoryScope {
                workspace_id,
                source: None,
                actor: None,
                agent_id: None,
                user_id: None,
                repo: Some("Quazmoz/memoryops".to_owned()),
            },
            memory_type,
            scope_visibility: ScopeVisibility::Private,
            content: format!("memory accessed {access_count} times"),
            entities: Json(Vec::<Entity>::new()),
            importance_score,
            importance_overridden: false,
            source_events: Vec::new(),
            embedding_id: None,
            token_count: None,
            decay_score: 1.0,
            relevance_score: 0.5,
            pinned,
            tags: Vec::new(),
            version: 1,
            promoted_at: None,
            source_episode_ids: Vec::new(),
            corroboration_count: 1,
            deleted_at: if deleted { Some(now) } else { None },
            last_accessed_at: None,
            created_at: now,
            updated_at: now,
        }
    }
}
