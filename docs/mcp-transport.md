# MCP Transport Options

MemoryOps MCP supports multiple transports.

## Client Quick-Reference

| Client | Config format | Transport | Guide |
|---|---|---|---|
| Open WebUI | Tool Server URL + Bearer token | `http` | [docs/integrations/openwebui.md](integrations/openwebui.md) |
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

| Tool | Purpose |
|---|---|
| `memory_observe` | Ingest a raw workspace observation for asynchronous consolidation into memory units. |
| `memory_list_observations` | List recent raw observations before processor consolidation. |
| `memory_list_contradictions` | List unresolved contradictions detected between memory units. |
| `memory_resolve_contradiction` | Resolve a contradiction by selecting keep/discard behavior. |

## Configuration

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
