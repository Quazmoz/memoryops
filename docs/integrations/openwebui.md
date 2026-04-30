# Connecting Open WebUI to MemoryOps

## Prerequisites

- MemoryOps running locally with Docker Compose services for Postgres, Redis, and Qdrant.
- Open WebUI installed and reachable from your browser.
- An API key from `POST /v1/workspaces`.
- The MCP server started with `MCP_TRANSPORT=http`.

## Start the MCP Server

Start the backing services and run migrations first:

```bash
docker compose up -d postgres redis qdrant
sqlx migrate run
```

For Docker Compose, run the MCP service with HTTP transport:

```bash
docker compose --profile mcp run --rm --service-ports \
  -e MCP_TRANSPORT=http \
  -e MCP_PORT=3003 \
  mcp
```

For local Rust development, run the MCP server directly:

```bash
MCP_TRANSPORT=http \
MCP_PORT=3003 \
DATABASE_URL=postgres://memoryops:memoryops@localhost:5432/memoryops \
REDIS_URL=redis://localhost:6379 \
QDRANT_URL=http://localhost:6333 \
cargo run -p mcp
```

The MCP endpoint will be available at `http://localhost:3003/mcp`.

## Add MemoryOps as an MCP Tool in Open WebUI

1. Open Open WebUI.
2. Go to **Settings** -> **Tools** -> **Add Tool Server**.
3. Set the server URL to `http://localhost:3003`.
4. Set auth to a bearer token using your MemoryOps API key.

Use this header value:

```text
Authorization: Bearer mops_...
```

![Open WebUI Add Tool Server dialog placeholder with callout arrows pointing to the Tool Server URL field, the Authorization Bearer token field, and the Save button.](openwebui-tool-server-placeholder.png)

## Test the Connection

Use Open WebUI's tool test panel to call `memory_retrieve`:

```json
{
  "query": "recent auth decisions",
  "limit": 5
}
```

A successful response includes a `memories` array and a `skills` array. Empty arrays are valid when no matching memories have been stored yet.

## Enabling Memory on a Model

Open the target model's settings in Open WebUI. Under **Tools**, enable the MemoryOps tools so Open WebUI can call them automatically during conversations.

## Troubleshooting

| Symptom | Check |
|---|---|
| CORS errors | MemoryOps MCP HTTP transport already uses `CorsLayer::permissive()`. Confirm the browser is pointing at the MCP server, not the API port. |
| Auth `401` | Verify the header is `Authorization: Bearer mops_...` and the key format is `mops_<prefix>_<32b>`. |
| Session errors | Set `MCP_TRANSPORT=http`; older `sse` transport can cause client session mismatches. |
| Connection refused | Confirm the MCP server is listening on port `3003` and Open WebUI can reach the host network. |
