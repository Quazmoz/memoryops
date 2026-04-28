use chrono::{DateTime, Utc};
use common::{
    error::AppResult,
    models::{MemoryScope, MemoryType},
    AppError, AppState,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use uuid::Uuid;

use super::ToolDefinition;

const DEFAULT_SOURCE: &str = "mcp";
const DEFAULT_IMPORTANCE: f32 = 0.5;

#[derive(Debug, Clone, Deserialize)]
pub struct StoreInput {
    pub content: String,
    #[serde(default = "default_source")]
    pub source: String,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default = "default_importance")]
    pub importance: f32,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct StoreOutput {
    pub id: Uuid,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, sqlx::FromRow)]
struct StoreOutputRow {
    id: Uuid,
    created_at: DateTime<Utc>,
}

pub fn definition() -> ToolDefinition {
    ToolDefinition {
        name: "memory_store",
        description: "Store an episodic MemoryOps memory for the authenticated workspace.",
        input_schema: json!({
            "type": "object",
            "required": ["content"],
            "properties": {
                "content": { "type": "string" },
                "source": { "type": "string", "default": DEFAULT_SOURCE },
                "tags": { "type": "array", "items": { "type": "string" } },
                "importance": { "type": "number", "minimum": 0.0, "maximum": 1.0, "default": DEFAULT_IMPORTANCE }
            }
        }),
    }
}

pub async fn run(
    state: &AppState,
    workspace_id: Uuid,
    input: StoreInput,
) -> AppResult<StoreOutput> {
    if input.content.trim().is_empty() {
        return Err(AppError::Validation("content is required".to_owned()));
    }
    if !(0.0..=1.0).contains(&input.importance) {
        return Err(AppError::Validation(
            "importance must be between 0.0 and 1.0".to_owned(),
        ));
    }

    let id = Uuid::now_v7();
    let source = normalized_source(&input.source);
    let scope = json!({
        "workspace_id": workspace_id,
        "agent_id": null,
        "user_id": null,
        "repo": null,
        "source": source
    });
    let _scope_shape: MemoryScope = serde_json::from_value(scope.clone())
        .map_err(|error| AppError::Validation(error.to_string()))?;
    let row = insert_memory_unit(state, id, workspace_id, scope, input, source).await?;

    let mut redis = state.redis.clone();
    processor::worker::enqueue_slow_job(&mut redis, row.id, workspace_id, 0).await?;

    Ok(StoreOutput {
        id: row.id,
        created_at: row.created_at,
    })
}

async fn insert_memory_unit(
    state: &AppState,
    id: Uuid,
    workspace_id: Uuid,
    scope: serde_json::Value,
    input: StoreInput,
    source: String,
) -> AppResult<StoreOutputRow> {
    sqlx::query_as::<_, StoreOutputRow>(
        r#"
        INSERT INTO memory_units (
            id,
            workspace_id,
            scope,
            memory_type,
            content,
            entities,
            importance_score,
            source_events,
            embedding_id,
            token_count,
            tags
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, NULL, NULL, $9)
        RETURNING id, created_at
        "#,
    )
    .bind(id)
    .bind(workspace_id)
    .bind(scope)
    .bind(MemoryType::Episodic)
    .bind(input.content)
    .bind(json!([{ "entity_type": "topic", "value": source, "confidence": 1.0 }]))
    .bind(input.importance)
    .bind(Vec::<Uuid>::new())
    .bind(input.tags)
    .fetch_one(&state.db)
    .await
    .map_err(AppError::Database)
}

fn normalized_source(source: &str) -> String {
    let trimmed = source.trim();
    if trimmed.is_empty() {
        DEFAULT_SOURCE.to_owned()
    } else {
        trimmed.to_owned()
    }
}

fn default_source() -> String {
    DEFAULT_SOURCE.to_owned()
}

fn default_importance() -> f32 {
    DEFAULT_IMPORTANCE
}
