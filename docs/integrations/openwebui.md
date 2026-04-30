# Connecting Open WebUI to MemoryOps

## Prerequisites

- MemoryOps stack running (see `[local-development.md](../local-development.md)`) with Postgres, Redis and Qdrant available.
- Open WebUI installed and reachable from your browser.
- A MemoryOps API key created via `POST /v1/workspaces` (one-time, saved when created).
- The MCP server started with `MCP_TRANSPORT=http` and reachable at `http://localhost:3003/mcp`.

Service ports (local dev): API `:8080`, MCP `:3003`, Frontend `:5173`, Postgres `5432`.

## Start the MCP server (quick)

Docker Compose (recommended for local dev):

```bash
docker compose up -d                       # starts postgres, redis, qdrant
sqlx migrate run
docker compose --profile mcp up -d        # starts MCP on port 3003
```

Local Rust (dev):

```bash
MCP_TRANSPORT=http \
MCP_PORT=3003 \
DATABASE_URL=postgres://memoryops:memoryops@localhost:5432/memoryops \
REDIS_URL=redis://localhost:6379 \
QDRANT_URL=http://localhost:6333 \
cargo run -p mcp
```

Note: the MCP HTTP Streamable endpoint is `http://localhost:3003/mcp` (include the `/mcp` path).

## 1) Add MemoryOps as a Tool Server in Open WebUI

1. Open Open WebUI and go to **Settings → Tools → Add Tool Server**.
2. Fill in the fields:
   - **Name:** MemoryOps
   - **URL:** `http://localhost:3003/mcp`  (must include `/mcp`)
   - **Auth:** Bearer Token
   - **Token:** your `mops_<prefix>_<32b>` API key
3. Save the tool server. Open WebUI should validate the URL and save the entry.

Authorization header (what MemoryOps expects):

```text
Authorization: Bearer mops_018f..._<32b>
```

## 2) Verify the connection (Open WebUI tool test)

- Open the newly added MemoryOps tool in Open WebUI and use the tool test panel.
- Paste this JSON into the test input for `memory_retrieve` and send:

```json
{
  "query": "recent auth decisions",
  "limit": 5
}
```

- Expected success: HTTP 200 and a JSON body containing `memories` and `retrieval_trace`. Example snippet:

```json
{
  "memories": [],
  "skills": [],
  "retrieval_trace": { "query_id": "trace-uuid", "strategy": "hybrid_rrf" }
}
```

Empty `memories` is valid (no ingested content yet). If you get a 401, check the token format and header.

## 3) Using MemoryOps in a conversation (enable tools)

1. Open the model configuration in Open WebUI.
2. Under **Tools**, enable the MemoryOps tool server and select which tools the model may call (at minimum `memory_retrieve` and `memory_store` / `memory_observe`).
3. Start a conversation. When the model decides to use a tool it will emit a tool call that Open WebUI routes to MemoryOps.

Tips:
- Prefer enabling `memory_retrieve` for read-only context and `memory_observe`/`memory_store` for explicit writes.
- Use short tool timeouts in the model config if you need fast fallbacks.

## 4) Storing memories from chat

There are two common approaches to persist information from a chat:

- Model-initiated tool call (`memory_store` or `memory_observe`): instruct the model to call the tool with structured input.
- Manual save via the Tools/Test panel: paste a `memory_store`/`memory_observe` payload and send.

Example `memory_observe` (raw workspace observation):

```json
{
  "workspace_id": "018f...",
  "source": "openwebui",
  "content": "Decision: migrate auth to workspace-scoped API keys",
  "tags": ["decision","auth"],
  "observed_at": "2026-04-30T12:34:00Z"
}
```

Example `memory_store` (explicit memory unit):

```json
{
  "memory_type": "semantic",
  "content": "Adopt workspace-scoped API keys for the auth service.",
  "tags": ["auth","api-keys","decision"],
  "workspace_id": "018f..."
}
```

## Worked example (multi-turn)

1. User: "I'm about to touch the auth code — show recent decisions about API keys."
2. Model → Tool Call: `memory_retrieve`

```json
{ "query": "auth api keys decision", "limit": 3 }
```

3. MemoryOps → Response: returns 2 memories describing past discussions.
4. Model: "Based on those, I recommend we adopt workspace-scoped API keys. I'll record that." → Model calls `memory_observe`:

```json
{ "workspace_id":"018f...", "source":"openwebui", "content":"Decision: adopt workspace-scoped API keys", "tags":["decision","auth"] }
```

5. MemoryOps → Response: 200 OK with `observation_id`.
6. User can re-run `memory_retrieve` to confirm the new observation is discoverable.

## Troubleshooting

| Symptom | Likely cause / action |
|---|---|
| CORS errors in browser console | MCP uses permissive CORS by default, but confirm the UI is calling `http://localhost:3003/mcp` and not another host. If behind a proxy, ensure the proxy passes headers through. See network tab for blocked headers. |
| 401 Unauthorized | Ensure header is exactly `Authorization: Bearer <your key>` and key matches `mops_..._<32b>`. Create a new workspace key via `POST /v1/workspaces` if needed. |
| Session mismatch / tool errors | Open WebUI expects an HTTP transport. Ensure `MCP_TRANSPORT=http` is set for the MCP process; using SSE or stdio will not work for the HTTP Streamable flow. See [mcp-transport.md](../mcp-transport.md). |
| Connection refused / cannot reach host | MCP not running on port 3003. Run `docker compose --profile mcp up -d` or start locally with `cargo run -p mcp`. Confirm `http://localhost:3003/mcp` responds. |
| Empty `memories` array | No ingested memories match the query — not an error. Ingest observations with `memory_observe` or `memory_store` and retry. |
| MCP_TRANSPORT unset | If unset the MCP binary may default to `stdio`; Open WebUI needs HTTP. Set `MCP_TRANSPORT=http` in the environment or Docker Compose profile. |

## References

- MCP transport and lifecycle: [mcp-transport.md](../mcp-transport.md)
- Local dev quickstart: [local-development.md](../local-development.md)
- API surface: [openapi.yaml](../openapi.yaml)

---
Terse checklist: add tool server → test `memory_retrieve` → enable tools on model → test write with `memory_observe`.
