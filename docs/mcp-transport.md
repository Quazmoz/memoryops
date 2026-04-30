# MCP Transport Options

MemoryOps MCP supports multiple transports.

| Transport | Status | Notes |
|---|---|---|
| `stdio` | Supported | Good for local process-hosted MCP usage. |
| `sse` | Deprecated | Legacy transport. Prefer HTTP Streamable. |
| `http` | Recommended | MCP 2025-03-26 style HTTP Streamable transport. |

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

## Spec Reference

Model Context Protocol specification (2025-03-26):

https://modelcontextprotocol.io/specification/2025-03-26
