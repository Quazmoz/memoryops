pub mod retrieve;
pub mod search;
pub mod store;

use chrono::{DateTime, Utc};
use retrieval::MemoryResult;
use serde::Serialize;
use serde_json::{json, Value};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ToolDefinition {
    pub name: &'static str,
    pub description: &'static str,
    pub input_schema: Value,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct MemoryToolResult {
    pub id: Uuid,
    pub content: String,
    pub memory_type: String,
    pub tags: Vec<String>,
    pub score: f32,
    pub importance_score: f32,
    pub created_at: DateTime<Utc>,
    pub source: String,
}

impl MemoryToolResult {
    pub fn from_memory_result(result: MemoryResult) -> Self {
        Self {
            id: result.memory.id,
            content: result.memory.content,
            memory_type: result.memory.memory_type,
            tags: result.memory.tags,
            score: result.score,
            importance_score: result.memory.importance_score,
            created_at: result.memory.created_at,
            source: source_from_scope(&result.memory.scope),
        }
    }
}

pub fn definitions() -> Vec<ToolDefinition> {
    vec![
        retrieve::definition(),
        search::definition(),
        store::definition(),
    ]
}

fn source_from_scope(scope: &Value) -> String {
    scope
        .get("source")
        .and_then(Value::as_str)
        .filter(|source| !source.trim().is_empty())
        .unwrap_or("memoryops")
        .to_owned()
}

pub fn memory_output_schema() -> Value {
    json!({
        "type": "array",
        "items": {
            "type": "object",
            "required": ["id", "content", "memory_type", "tags", "score", "importance_score", "created_at", "source"],
            "properties": {
                "id": { "type": "string", "format": "uuid" },
                "content": { "type": "string" },
                "memory_type": { "type": "string", "enum": ["episodic", "semantic"] },
                "tags": { "type": "array", "items": { "type": "string" } },
                "score": { "type": "number" },
                "importance_score": { "type": "number" },
                "created_at": { "type": "string", "format": "date-time" },
                "source": { "type": "string" }
            }
        }
    })
}
