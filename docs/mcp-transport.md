# MCP Transport Options

MemoryOps MCP supports multiple transports.

## Client Quick-Reference

| Client | Config format | Transport | Guide |
|---|---|---|---|
| Open WebUI | Tool Server URL + Bearer token | `http` | [docs/integrations/openwebui.md](integrations/openwebui.md) |
| Claude Code | `.mcp.json` in project root | `http` or `stdio` | [docs/integrations/claude-code.md](integrations/claude-code.md) |
| GitHub Copilot (VS Code) | `.vscode/mcp.json` | `http` or `stdio` | [docs/integrations/vscode.md](integrations/vscode.md) |
| Continue.dev | `~/.continue/config.json` | `http` | [docs/integrations/vscode.md](integrations/vscode.md) |
| Claude Desktop | `claude_desktop_config.json` | `stdio` | (see below) |
| Custom agent | `POST /mcp` + `Authorization: Bearer` | `http` | This file |

## Transport Matrix

| Transport | Status | Notes |
|---|---|---|
| `stdio` | Supported | Good for local process-hosted MCP usage. |
| `sse` | Deprecated | Legacy transport. Prefer HTTP Streamable. |
| `http` | Recommended | MCP 2025-03-26 style HTTP Streamable transport. |

## Tools Reference

All 12 tools are available over any transport. `workspace_id` is always injected from 
the authenticated MCP session and is never a tool parameter.

| Tool | Purpose |
|---|---|
| `memory_retrieve` | Token-budget-aware hybrid retrieval. Returns scored, token-packed memories (default budget: 4096 tokens). |
| `memory_search` | Filtered search by tags or `memory_type` without token-budget packing. |
| `memory_store` | Directly persist an episodic memory. Immediate — bypasses the observation queue. |
| `memory_observe` | Ingest a raw observation for async classification by the processor. |
| `skill_invoke` | Invoke a registered workspace skill using the same rate-limit, circuit-breaker, audit, and invocation-log path as the HTTP API. |
| `memory_update` | Update `content`, `tags`, or `importance_score` on an existing memory unit. |
| `memory_delete` | Soft-delete a memory and remove its Qdrant vector point. |
| `memory_feedback` | Submit a relevance rating (`-1`/`0`/`1`) on a retrieved memory to bias future scoring. |
| `memory_timeline` | Retrieve memories as they existed at a specific past timestamp (`as_of`). |
| `memory_list_observations` | List raw observations queued but not yet consolidated by the processor. |
| `memory_list_contradictions` | List unresolved contradictions detected between memory units. |
| `memory_resolve_contradiction` | Resolve a contradiction: `keep_a`, `keep_b`, `keep_both`, or `discard_both`. |

## Configuration

> **Warning:** The `mcp` service in `docker-compose.yml` defaults to
> `MCP_TRANSPORT: "sse"`, which is the deprecated transport. Always override
> it when starting the MCP container:
>
> ```bash
> docker compose --profile mcp run --rm --service-ports \
>   -e MCP_TRANSPORT=http \
>   -e MCP_PORT=3003 \
>   mcp
> ```

Set transport with:

```bash
export MCP_TRANSPORT=http
```

HTTP transports listen on `MCP_PORT` (default `3003`).

## HTTP Streamable Lifecycle

Single endpoint: `/mcp`

1. `POST /mcp` initialize (no session header yet):
- Server creates a session.
- Server returns `Mcp-Session-Id: <uuid>`.

2. `POST /mcp` subsequent requests:
- Include `Mcp-Session-Id` header.
- Include `Authorization: Bearer <api_key>`.
- Server responds with JSON (`application/json`) for normal request/response calls.
- Notifications (JSON-RPC message without `id`) return `202 Accepted`.

3. `GET /mcp`:
- Include `Mcp-Session-Id`.
- Opens SSE stream for server-initiated messages/keep-alive.

4. `DELETE /mcp`:
- Include `Mcp-Session-Id`.
- Server closes and removes the session.

## Minimal curl Flow

Initialize:

```bash
curl -i -X POST http://localhost:3003/mcp \
  -H 'Authorization: Bearer <api_key>' \
  -H 'Content-Type: application/json' \
  -d '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}'
```

Capture `Mcp-Session-Id` from response headers, then call a tool:

```bash
curl -sS -X POST http://localhost:3003/mcp \
  -H 'Authorization: Bearer <api_key>' \
  -H 'Mcp-Session-Id: <session_id>' \
  -H 'Content-Type: application/json' \
  -d '{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}'
```

Delete session:

```bash
curl -sS -X DELETE http://localhost:3003/mcp \
  -H 'Mcp-Session-Id: <session_id>'
```

## Claude Desktop

Use `stdio` transport when Claude Desktop launches MemoryOps as a local process:

```jsonc
// ~/Library/Application Support/Claude/claude_desktop_config.json
{
  "mcpServers": {
    "memoryops": {
      "command": "cargo",
      "args": ["run", "--manifest-path", "/path/to/memoryops/Cargo.toml", "-p", "mcp"],
      "env": {
        "MCP_TRANSPORT": "stdio",
        "DATABASE_URL": "postgres://...",
        "REDIS_URL": "redis://localhost:6379",
        "QDRANT_URL": "http://localhost:6334"
      }
    }
  }
}
```

## Spec Reference

Model Context Protocol specification (2025-03-26):

https://modelcontextprotocol.io/specification/2025-03-26
