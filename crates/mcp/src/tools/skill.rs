use std::collections::HashMap;

use common::{
    auth::AuthContext,
    error::AppResult,
    services::{invoke_workspace_skill, SkillInvocationSource},
    AppError, AppState,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use super::ToolDefinition;

#[derive(Debug, Clone, Deserialize)]
pub struct SkillInvokeInput {
    pub name: String,
    pub body: Option<Value>,
    pub headers: Option<HashMap<String, String>>,
    pub version: Option<i32>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct SkillInvokeOutput {
    pub name: String,
    pub version: i32,
    pub status: u16,
    pub latency_ms: u64,
    pub body: Value,
}

pub fn definition() -> ToolDefinition {
    ToolDefinition {
        name: "skill_invoke",
        description: "Invoke an enabled MemoryOps skill for the authenticated workspace.",
        input_schema: json!({
            "type": "object",
            "required": ["name"],
            "description": "workspace_id is injected from the authenticated MCP session, not accepted in tool input.",
            "properties": {
                "name": { "type": "string", "minLength": 1, "maxLength": 64 },
                "body": {
                    "description": "Optional JSON request body to send to the skill endpoint."
                },
                "headers": {
                    "type": "object",
                    "additionalProperties": { "type": "string" },
                    "description": "Optional caller headers. Sensitive and transport headers are filtered server-side."
                },
                "version": {
                    "type": "integer",
                    "description": "Optional specific version of the skill to invoke. If omitted, the active version is used."
                }
            }
        }),
    }
}

pub async fn run(
    state: &AppState,
    context: &AuthContext,
    input: SkillInvokeInput,
) -> AppResult<SkillInvokeOutput> {
    let name = input.name.trim();
    if name.is_empty() {
        return Err(AppError::Validation("name is required".to_owned()));
    }

    let actor = context.actor();
    let (response, version) = invoke_workspace_skill(
        state,
        context.workspace_id,
        name,
        input.body.as_ref(),
        input.headers.as_ref(),
        SkillInvocationSource::Mcp,
        &actor,
        input.version,
    )
    .await?;

    Ok(SkillInvokeOutput {
        name: name.to_owned(),
        version,
        status: response.status,
        latency_ms: response.latency_ms,
        body: response.body,
    })
}