use std::sync::Arc;

use async_trait::async_trait;
use common::{auth::AuthContext, AppError, AppState};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio::sync::RwLock;

use crate::{auth, tools, MCP_PROTOCOL_VERSION};

const PARSE_ERROR: i32 = -32700;
const INVALID_REQUEST: i32 = -32600;
const METHOD_NOT_FOUND: i32 = -32601;
const INVALID_PARAMS: i32 = -32602;
const INTERNAL_ERROR: i32 = -32603;
const UNAUTHORIZED: i32 = -32001;

// rmcp 1.5.0 is the current stable official SDK, but its published docs track
// a newer MCP spec and did not clearly document 2025-06-18 HTTP SSE support
// when M11 was implemented. Keep this small JSON-RPC envelope local for now.
// TODO: revisit rmcp after https://github.com/modelcontextprotocol/rust-sdk/discussions/716
// clarifies 1.x transport and protocol-version pinning.

#[async_trait]
pub trait McpBackend: Send + Sync + 'static {
    async fn authenticate(&self, token: &str) -> Result<AuthContext, JsonRpcError>;

    async fn memory_retrieve(
        &self,
        context: &AuthContext,
        input: tools::retrieve::RetrieveInput,
    ) -> Result<tools::retrieve::RetrieveOutput, JsonRpcError>;

    async fn memory_search(
        &self,
        context: &AuthContext,
        input: tools::search::SearchInput,
    ) -> Result<Vec<tools::MemoryToolResult>, JsonRpcError>;

    async fn memory_store(
        &self,
        context: &AuthContext,
        input: tools::store::StoreInput,
    ) -> Result<tools::store::StoreOutput, JsonRpcError>;
}

#[derive(Clone)]
pub struct RuntimeBackend {
    state: AppState,
}

impl RuntimeBackend {
    pub fn new(state: AppState) -> Self {
        Self { state }
    }
}

#[async_trait]
impl McpBackend for RuntimeBackend {
    async fn authenticate(&self, token: &str) -> Result<AuthContext, JsonRpcError> {
        let context = common::auth::validate_api_key(&self.state.db, token)
            .await
            .map_err(auth_error_to_rpc)?;
        common::auth::spawn_last_used_update(self.state.db.clone(), context.key_id);
        Ok(context)
    }

    async fn memory_retrieve(
        &self,
        context: &AuthContext,
        input: tools::retrieve::RetrieveInput,
    ) -> Result<tools::retrieve::RetrieveOutput, JsonRpcError> {
        tools::retrieve::run(&self.state, context.workspace_id, input)
            .await
            .map_err(app_error_to_rpc)
    }

    async fn memory_search(
        &self,
        context: &AuthContext,
        input: tools::search::SearchInput,
    ) -> Result<Vec<tools::MemoryToolResult>, JsonRpcError> {
        tools::search::run(&self.state, context.workspace_id, input)
            .await
            .map_err(app_error_to_rpc)
    }

    async fn memory_store(
        &self,
        context: &AuthContext,
        input: tools::store::StoreInput,
    ) -> Result<tools::store::StoreOutput, JsonRpcError> {
        tools::store::run(&self.state, context.workspace_id, input)
            .await
            .map_err(app_error_to_rpc)
    }
}

pub type SharedServer = Arc<McpServer<RuntimeBackend>>;

pub struct McpServer<B: McpBackend> {
    backend: Arc<B>,
    session: RwLock<Option<AuthContext>>,
    tools: Vec<tools::ToolDefinition>,
}

impl<B: McpBackend> McpServer<B> {
    pub fn new(backend: B) -> Self {
        Self {
            backend: Arc::new(backend),
            session: RwLock::new(None),
            tools: tools::definitions(),
        }
    }

    pub fn with_backend(backend: Arc<B>) -> Self {
        Self {
            backend,
            session: RwLock::new(None),
            tools: tools::definitions(),
        }
    }

    pub async fn handle_json_line(&self, line: &str) -> Option<String> {
        let value = match serde_json::from_str::<Value>(line) {
            Ok(value) => value,
            Err(error) => {
                let response = error_response(
                    Some(Value::Null),
                    JsonRpcError::new(PARSE_ERROR, format!("Parse error: {error}")),
                );
                return Some(response.to_string());
            }
        };

        self.handle_message(value)
            .await
            .map(|response| response.to_string())
    }

    pub async fn handle_message(&self, message: Value) -> Option<Value> {
        tracing::trace!(message = %message, "received MCP JSON-RPC message");
        let should_respond = message.get("id").is_some();
        let id = message.get("id").cloned();
        let request = match serde_json::from_value::<JsonRpcRequest>(message) {
            Ok(request) => request,
            Err(error) => {
                return Some(error_response(
                    id.or(Some(Value::Null)),
                    JsonRpcError::new(INVALID_REQUEST, format!("Invalid request: {error}")),
                ));
            }
        };

        if request.jsonrpc.as_deref() != Some("2.0") {
            if should_respond {
                return Some(error_response(
                    id,
                    JsonRpcError::new(INVALID_REQUEST, "jsonrpc must be 2.0"),
                ));
            }
            return None;
        }

        let result = self.dispatch(&request.method, request.params).await;
        if !should_respond {
            return None;
        }

        Some(match result {
            Ok(result) => success_response(id, result),
            Err(error) => error_response(id, error),
        })
    }

    pub async fn handle_http_message(&self, message: Value, bearer_token: Option<String>) -> Value {
        let id = message.get("id").cloned();
        if let Some(token) = bearer_token {
            if let Err(error) = self.authenticate_session(&token).await {
                return error_response(id, error);
            }
        }

        self.handle_message(message)
            .await
            .unwrap_or_else(|| success_response(id, json!({})))
    }

    async fn dispatch(&self, method: &str, params: Option<Value>) -> Result<Value, JsonRpcError> {
        match method {
            "initialize" => self.initialize(params).await,
            "tools/list" => Ok(json!({ "tools": &self.tools })),
            "tools/call" => self.call_tool(params).await,
            "ping" => Ok(json!({})),
            _ => Err(JsonRpcError::new(
                METHOD_NOT_FOUND,
                format!("Method not found: {method}"),
            )),
        }
    }

    async fn initialize(&self, params: Option<Value>) -> Result<Value, JsonRpcError> {
        let context = match auth::initialize_bearer_token(params.as_ref()) {
            Some(token) => self.authenticate_session(&token).await?,
            None => self
                .session_context()
                .await
                .ok_or_else(JsonRpcError::unauthorized)?,
        };

        Ok(json!({
            "protocolVersion": MCP_PROTOCOL_VERSION,
            "serverInfo": {
                "name": "memoryops-mcp",
                "version": env!("CARGO_PKG_VERSION")
            },
            "capabilities": {
                "tools": { "listChanged": false }
            },
            "workspace": {
                "id": context.workspace_id
            },
            "tools": &self.tools
        }))
    }

    async fn authenticate_session(&self, token: &str) -> Result<AuthContext, JsonRpcError> {
        let context = self.backend.authenticate(token).await?;
        *self.session.write().await = Some(context.clone());
        Ok(context)
    }

    async fn session_context(&self) -> Option<AuthContext> {
        self.session.read().await.clone()
    }

    async fn call_tool(&self, params: Option<Value>) -> Result<Value, JsonRpcError> {
        let params = params.ok_or_else(|| JsonRpcError::new(INVALID_PARAMS, "params required"))?;
        let call = serde_json::from_value::<ToolCallParams>(params)
            .map_err(|error| JsonRpcError::new(INVALID_PARAMS, error.to_string()))?;
        let context = self
            .session_context()
            .await
            .ok_or_else(JsonRpcError::unauthorized)?;

        let value = match call.name.as_str() {
            "memory_retrieve" => {
                let input =
                    serde_json::from_value::<tools::retrieve::RetrieveInput>(call.arguments)
                        .map_err(|error| JsonRpcError::new(INVALID_PARAMS, error.to_string()))?;
                serde_json::to_value(self.backend.memory_retrieve(&context, input).await?)
            }
            "memory_search" => {
                let input = serde_json::from_value::<tools::search::SearchInput>(call.arguments)
                    .map_err(|error| JsonRpcError::new(INVALID_PARAMS, error.to_string()))?;
                serde_json::to_value(self.backend.memory_search(&context, input).await?)
            }
            "memory_store" => {
                let input = serde_json::from_value::<tools::store::StoreInput>(call.arguments)
                    .map_err(|error| JsonRpcError::new(INVALID_PARAMS, error.to_string()))?;
                serde_json::to_value(self.backend.memory_store(&context, input).await?)
            }
            _ => return Err(JsonRpcError::new(INVALID_PARAMS, "unknown tool name")),
        }
        .map_err(|error| JsonRpcError::new(INTERNAL_ERROR, error.to_string()))?;

        Ok(tool_result(value))
    }
}

#[derive(Debug, Deserialize)]
struct JsonRpcRequest {
    jsonrpc: Option<String>,
    method: String,
    params: Option<Value>,
}

#[derive(Debug, Deserialize)]
struct ToolCallParams {
    name: String,
    #[serde(default)]
    arguments: Value,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct JsonRpcError {
    pub code: i32,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

impl JsonRpcError {
    pub fn new(code: i32, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            data: None,
        }
    }

    pub fn unauthorized() -> Self {
        Self::new(UNAUTHORIZED, "Unauthorized")
    }
}

fn success_response(id: Option<Value>, result: Value) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id.unwrap_or(Value::Null),
        "result": result
    })
}

fn error_response(id: Option<Value>, error: JsonRpcError) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id.unwrap_or(Value::Null),
        "error": error
    })
}

fn tool_result(value: Value) -> Value {
    json!({
        "content": [
            {
                "type": "text",
                "text": value.to_string()
            }
        ],
        "structuredContent": value,
        "isError": false
    })
}

fn auth_error_to_rpc(error: AppError) -> JsonRpcError {
    match error {
        AppError::Unauthorized => JsonRpcError::unauthorized(),
        other => {
            tracing::error!(error = ?other, "MCP auth failed");
            JsonRpcError::new(INTERNAL_ERROR, "Internal error")
        }
    }
}

fn app_error_to_rpc(error: AppError) -> JsonRpcError {
    match error {
        AppError::Unauthorized => JsonRpcError::unauthorized(),
        AppError::Validation(message) => JsonRpcError::new(INVALID_PARAMS, message),
        other => {
            tracing::error!(error = ?other, "MCP tool failed");
            JsonRpcError::new(INTERNAL_ERROR, "Internal error")
        }
    }
}

#[cfg(test)]
mod tests {
    use chrono::Utc;
    use serde_json::json;
    use uuid::Uuid;

    use super::*;

    #[derive(Default)]
    struct MockBackend {
        store_calls: std::sync::atomic::AtomicUsize,
    }

    #[async_trait]
    impl McpBackend for MockBackend {
        async fn authenticate(&self, token: &str) -> Result<AuthContext, JsonRpcError> {
            if token != "valid-token" {
                return Err(JsonRpcError::unauthorized());
            }

            Ok(AuthContext {
                workspace_id: Uuid::from_u128(7),
                key_id: Uuid::from_u128(8),
                key_prefix: "mops_012".to_owned(),
            })
        }

        async fn memory_retrieve(
            &self,
            _context: &AuthContext,
            _input: tools::retrieve::RetrieveInput,
        ) -> Result<tools::retrieve::RetrieveOutput, JsonRpcError> {
            Ok(tools::retrieve::RetrieveOutput {
                memories: vec![memory_result(0.91)],
                skills: vec![tools::SkillToolResult {
                    name: "summarize_pr".to_owned(),
                    description: "Summarize pull requests".to_owned(),
                    endpoint_url: "https://example.com/summarize".to_owned(),
                    http_method: "POST".to_owned(),
                    input_schema: json!({}),
                    output_schema: json!({}),
                }],
            })
        }

        async fn memory_search(
            &self,
            _context: &AuthContext,
            _input: tools::search::SearchInput,
        ) -> Result<Vec<tools::MemoryToolResult>, JsonRpcError> {
            Ok(vec![memory_result(0.77)])
        }

        async fn memory_store(
            &self,
            _context: &AuthContext,
            _input: tools::store::StoreInput,
        ) -> Result<tools::store::StoreOutput, JsonRpcError> {
            self.store_calls
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Ok(tools::store::StoreOutput {
                id: Uuid::from_u128(11),
                created_at: Utc::now(),
            })
        }
    }

    fn memory_result(score: f32) -> tools::MemoryToolResult {
        tools::MemoryToolResult {
            id: Uuid::from_u128(10),
            content: "memory content".to_owned(),
            memory_type: "episodic".to_owned(),
            tags: vec!["mcp".to_owned()],
            score,
            importance_score: 0.8,
            created_at: Utc::now(),
            source: "mcp".to_owned(),
        }
    }

    async fn initialized_server() -> McpServer<MockBackend> {
        let server = McpServer::new(MockBackend::default());
        let response = match server
            .handle_message(json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "initialize",
                "params": { "_meta": { "auth": { "token": "Bearer valid-token" } } }
            }))
            .await
        {
            Some(response) => response,
            None => panic!("initialize should respond"),
        };
        assert!(response.get("result").is_some());
        server
    }

    #[tokio::test]
    async fn tools_list_contains_exactly_three_tools() {
        let server = McpServer::new(MockBackend::default());
        let response = match server
            .handle_message(json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "tools/list"
            }))
            .await
        {
            Some(response) => response,
            None => panic!("tools/list should respond"),
        };
        let tools = match response
            .get("result")
            .and_then(|result| result.get("tools"))
            .and_then(Value::as_array)
        {
            Some(tools) => tools,
            None => panic!("tools/list should return tools array"),
        };
        let names = tools
            .iter()
            .filter_map(|tool| tool.get("name").and_then(Value::as_str))
            .collect::<Vec<_>>();

        assert_eq!(
            names,
            vec!["memory_retrieve", "memory_search", "memory_store"]
        );
    }

    #[tokio::test]
    async fn memory_retrieve_returns_json_shape() {
        let server = initialized_server().await;
        let response = call_tool(
            &server,
            "memory_retrieve",
            json!({ "query": "memory", "limit": 1 }),
        )
        .await;
        let structured = structured_content(&response);
        let first = structured
            .get("memories")
            .and_then(Value::as_array)
            .and_then(|items| items.first())
            .expect("memory_retrieve should include memories");

        assert!(first.get("id").is_some());
        assert!(first.get("content").is_some());
        assert!(first.get("score").is_some());
        assert!(structured
            .get("skills")
            .and_then(Value::as_array)
            .is_some_and(|items| !items.is_empty()));
    }

    #[tokio::test]
    async fn memory_search_keyword_returns_json_shape() {
        let server = initialized_server().await;
        let response = call_tool(
            &server,
            "memory_search",
            json!({ "query": "memory", "search_type": "keyword" }),
        )
        .await;
        let first = first_structured_item(&response);

        assert!(first.get("id").is_some());
        assert!(first.get("content").is_some());
        assert!(first.get("score").is_some());
    }

    #[tokio::test]
    async fn memory_store_returns_id_and_created_at() {
        let backend = Arc::new(MockBackend::default());
        let server = McpServer::with_backend(backend.clone());
        let init = server
            .handle_message(json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "initialize",
                "params": { "_meta": { "auth": { "token": "Bearer valid-token" } } }
            }))
            .await;
        assert!(init.is_some());

        let response = call_tool(
            &server,
            "memory_store",
            json!({ "content": "agent-authored memory" }),
        )
        .await;
        let structured = structured_content(&response);

        assert!(structured.get("id").is_some());
        assert!(structured.get("created_at").is_some());
        assert_eq!(
            backend
                .store_calls
                .load(std::sync::atomic::Ordering::SeqCst),
            1
        );
    }

    #[tokio::test]
    async fn missing_token_returns_unauthorized_error() {
        let server = McpServer::new(MockBackend::default());
        let response = match server
            .handle_message(json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "initialize",
                "params": {}
            }))
            .await
        {
            Some(response) => response,
            None => panic!("initialize should respond"),
        };

        assert_eq!(error_code(&response), UNAUTHORIZED);
    }

    #[tokio::test]
    async fn invalid_token_returns_unauthorized_error() {
        let server = McpServer::new(MockBackend::default());
        let response = match server
            .handle_message(json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "initialize",
                "params": { "_meta": { "auth": { "token": "Bearer invalid" } } }
            }))
            .await
        {
            Some(response) => response,
            None => panic!("initialize should respond"),
        };

        assert_eq!(error_code(&response), UNAUTHORIZED);
    }

    #[tokio::test]
    async fn ping_returns_empty_result_object() {
        let server = McpServer::new(MockBackend::default());
        let response = match server
            .handle_message(json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "ping"
            }))
            .await
        {
            Some(response) => response,
            None => panic!("ping should respond"),
        };

        assert_eq!(response.get("result"), Some(&json!({})));
    }

    async fn call_tool(server: &McpServer<MockBackend>, name: &str, arguments: Value) -> Value {
        match server
            .handle_message(json!({
                "jsonrpc": "2.0",
                "id": 2,
                "method": "tools/call",
                "params": {
                    "name": name,
                    "arguments": arguments
                }
            }))
            .await
        {
            Some(response) => response,
            None => panic!("tools/call should respond"),
        }
    }

    fn first_structured_item(response: &Value) -> &Value {
        match structured_content(response)
            .as_array()
            .and_then(|items| items.first())
        {
            Some(first) => first,
            None => panic!("structuredContent should contain at least one item"),
        }
    }

    fn structured_content(response: &Value) -> &Value {
        match response
            .get("result")
            .and_then(|result| result.get("structuredContent"))
        {
            Some(value) => value,
            None => panic!("tool response should include structuredContent"),
        }
    }

    fn error_code(response: &Value) -> i32 {
        match response
            .get("error")
            .and_then(|error| error.get("code"))
            .and_then(Value::as_i64)
            .and_then(|code| i32::try_from(code).ok())
        {
            Some(code) => code,
            None => panic!("response should contain JSON-RPC error code"),
        }
    }
}
