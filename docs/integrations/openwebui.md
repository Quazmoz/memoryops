## Open WebUI — MemoryOps integration

This guide shows how to connect Open WebUI to a local MemoryOps deployment via the MCP HTTP Streamable transport, verify the connection, and run a full memory read/write round-trip.

## Prerequisites

- MemoryOps API server running on :8080
- Docker Compose infra up (postgres, redis, qdrant)
- Migrations applied: `sqlx migrate run`
- MCP server started with `MCP_TRANSPORT=http` and bound to localhost/private networking.
- Open WebUI installed and reachable from your browser
- API key obtained from `POST /v1/workspaces` (created once at workspace bootstrap)

Create a workspace (example):

```bash
# Create workspace — returns workspace_id + api_key in a single call
curl -sS -X POST http://localhost:8080/v1/workspaces \
  -H 'Content-Type: application/json' \
  -d '{"name": "my-workspace"}'
# Response: {"workspace_id": "YOUR_WORKSPACE_ID", "api_key": "YOUR_MEMORYOPS_API_KEY"}
```

Service ports (local dev): API `:8080`, MCP `:3003`, Frontend `:5173`, Postgres `:5432`, Redis `:6379`, Qdrant HTTP `:6333`, Qdrant gRPC `:6334`.

## Start MCP With Private HTTP Transport

The compose service uses HTTP transport and binds to `127.0.0.1:3003` by default. Keep it local/private; do not expose MCP directly to the internet. To start an explicit local MCP container:

```bash
# Local/private HTTP transport
docker compose --profile mcp run --rm --service-ports \
  -e MCP_TRANSPORT=http \
  -e MCP_PORT=3003 \
  mcp
```

Or for local Rust development (no Docker for MCP):

```bash
MCP_TRANSPORT=http \
MCP_PORT=3003 \
DATABASE_URL=postgres://memoryops:memoryops@localhost:5432/memoryops \
REDIS_URL=redis://localhost:6379 \
QDRANT_URL=http://localhost:6334 \
cargo run -p mcp
```

When the MCP server is running it is reachable at: `http://localhost:3003/mcp`

## Start the MCP Server

### Docker Compose (with transport override)

Use the explicit local command shown above when you want the MCP process available to Open WebUI:

```bash
docker compose --profile mcp run --rm --service-ports \
  -e MCP_TRANSPORT=http \
  -e MCP_PORT=3003 \
  mcp
```

### Local Rust development

If you prefer running the MCP binary locally for development, set the environment and run the service directly:

```bash
MCP_TRANSPORT=http \
MCP_PORT=3003 \
DATABASE_URL=postgres://memoryops:memoryops@localhost:5432/memoryops \
REDIS_URL=redis://localhost:6379 \
QDRANT_URL=http://localhost:6334 \
cargo run -p mcp
```

## Connect Open WebUI

1. Open WebUI → Settings → Tools → Add Tool Server (label may be “MCP Servers” in some versions).
2. URL field: `http://localhost:3003` (note: newer Open WebUI versions append `/mcp` automatically — verify your OWUI version; try both `http://localhost:3003` and `http://localhost:3003/mcp` if tools fail to appear).
3. Auth: Bearer token — supply your API key as `Authorization: Bearer mops_<prefix>_<hex32>` (the alternative header `X-API-Key: mops_...` also works).
4. Save and verify the tool list loads.

Authorization header example:

```text
Authorization: Bearer YOUR_MEMORYOPS_API_KEY
```

## Verify the Connection

Use the Open WebUI tool test panel to call `memory_retrieve` with this payload:

```json
{ "query": "recent decisions", "limit": 5 }
```

An HTTP 200 with a JSON body containing `memories` is expected. An empty `memories: []` is valid when no memories exist yet — not an error.

## Enable MemoryOps on a Model

1. Open the model configuration in Open WebUI.
2. Under **Tools** enable the MemoryOps tool server you added.
3. Select which tools the model may call (recommended minimum: `memory_retrieve`, plus `memory_store`/`memory_observe` to enable writes).
4. Save and start a conversation. The model will call enabled tools automatically when it decides they are needed.

## Worked Example: Full Memory Round-Trip

> NOTE: `workspace_id` is NEVER a tool parameter — it is injected from the authenticated MCP session and must not be included in tool inputs.

1. User: “Show recent decisions about the auth service.”

2. Model → Tool Call: `memory_retrieve`

```json
{ "query": "auth service decisions", "limit": 5 }
```

3. MemoryOps → Response:

```json
{
  "memories": [
    {
      "id": "3f9a...",
      "content": "Adopt workspace-scoped API keys for service-to-service calls.",
      "memory_type": "semantic",
      "tags": ["auth","api-keys"],
      "score": 0.87,
      "importance_score": 0.72,
      "created_at": "2026-04-28T10:00:00Z",
      "source": "github"
    }
  ],
  "skills": []
}
```

4. Model uses that context in its reply to the user.

5. User confirms the model's recommendation is correct.

6. Model logs the confirmed fact via `memory_observe` (async processing path):

```json
{
  "content": "Confirmed: adopt workspace-scoped API keys for service-to-service calls.",
  "source": "openwebui",
  "tags": ["decision","auth"]
}
```

MemoryOps → Response: `{"id":"...","status":"queued"}`

7. Inspect queued observations with `memory_list_observations`. If `processed_at: null` it means the item is queued and awaiting processor work — this is normal, not an error.

## `memory_store` vs `memory_observe` — When to Use Each

- `memory_store`: direct, immediate persistence of a memory unit. Use for well-formed, high-confidence decisions you want available for immediate retrieval.
- `memory_observe`: submits a raw observation into the async processor pipeline. Use for unstructured notes or things you want the processor to classify/score.

Rule of thumb: use `memory_store` for important, retrievable decisions; use `memory_observe` for noisy or raw content you want the system to consolidate.

## Inline Feedback Pattern

You can submit feedback ratings inline in the same `memory_retrieve` call. This is preferred over a separate `memory_feedback` call.

Example:

```json
{
  "query": "recent auth decisions",
  "limit": 5,
  "feedback": {
    "query_id": "previous-trace-uuid",
    "ratings": [
      { "memory_id": "3f9a...", "rating": 1 }
    ]
  }
}
```

## Troubleshooting

| Symptom | Root Cause | Fix |
|---|---|---|
| CORS error in browser | — | MCP already sets `CorsLayer::permissive()`. Check you're pointing at port 3003, not 8080. |
| `401 Unauthorized` | Wrong header or key format | Header must be `Authorization: Bearer mops_<prefix>_<hex32>`. `X-API-Key: mops_...` also works. |
| Session errors / `Mcp-Session-Id` rejected | Wrong transport or stale session | Set `MCP_TRANSPORT=http` when starting MCP and retry initialization. |
| Connection refused on port 3003 | MCP profile not running | Run `docker compose --profile mcp ...` with transport override, or `cargo run -p mcp` with env vars set. |
| `memories: []` in response | No memories ingested yet | Valid. Seed data: `API_KEY=... bash scripts/seed.sh`, or ingest via `memory_store` / `memory_observe`. |
| Tools not showing in Open WebUI | OWUI version quirk | Some OWUI versions expect `/mcp` appended to the URL, others don't. Try both `http://localhost:3003` and `http://localhost:3003/mcp`. |
| `processed_at: null` in list_observations | Job queued, not yet processed | Normal. Processor workers run async. Check `RUST_LOG=debug` for worker activity. If stuck, check Ollama is running (`ollama serve`). |

## References

- [mcp-transport.md](../mcp-transport.md)
- [local-development.md](../local-development.md)
- [bootstrap.md](../bootstrap.md)
- [openapi.yaml](../openapi.yaml)

---

Terse checklist: add tool server → test `memory_retrieve` → enable tools on model → test write with `memory_observe`.
