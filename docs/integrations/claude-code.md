# Claude Code: MemoryOps MCP integration

Short guide for configuring Anthropic's Claude Code (`claude`) to use MemoryOps as an MCP tool server. Prefer a project-scoped `.mcp.json` in this repo (`.claude/` exists here) so other contributors can use the same settings without leaking global config.

## Prerequisites

- MemoryOps running locally (see `[local-development.md](../local-development.md)`).
- MCP server available (HTTP or stdio) and/or `cargo` in PATH if using stdio spawn.
- A MemoryOps API key (`mops_<prefix>_<32b>`) from `POST /v1/workspaces`.

## Config options

Claude Code supports configuring MCP servers either globally (`~/.claude.json`) or per-project (`.mcp.json` in repo root). We recommend project-scoped config for MemoryOps in this repository.

### Project-scoped (recommended)

Create a `.mcp.json` in the project root (or update an existing one). HTTP transport example:

```json
{
  "mcpServers": {
    "memoryops": {
      "type": "http",
      "url": "http://localhost:3003/mcp",
      "headers": {
        "Authorization": "Bearer YOUR_API_KEY"
      }
    }
  }
}
```

StdIO transport example (Claude Code will spawn the process):

```json
{
  "mcpServers": {
    "memoryops": {
      "type": "stdio",
      "command": "cargo",
      "args": ["run", "-p", "mcp"],
      "env": {
        "MCP_TRANSPORT": "stdio",
        "DATABASE_URL": "postgres://memoryops:memoryops@localhost:5432/memoryops",
        "REDIS_URL": "redis://localhost:6379",
        "QDRANT_URL": "http://localhost:6334"
      }
    }
  }
}
```

Notes:
- HTTP: Claude Code sends requests to `http://localhost:3003/mcp` and expects an HTTP Streamable MCP server.
- stdio: Claude Code spawns the command and communicates over stdio. Ensure `cargo` is available and the environment variables are correct.

### Global config (~/.claude.json)

If you prefer a global server for local experiments, add an `mcpServers` entry to `~/.claude.json` using the same shape as `.mcp.json` above. Project-scoped configuration takes precedence when present.

## Verify the connection

Two ways to verify: direct MCP HTTP smoke test, and an in-`claude` check.

### A — Simple HTTP smoke test (confirms MemoryOps MCP is reachable)

1. Ensure MCP is running on `:3003` (Docker: `docker compose --profile mcp up -d`, or start locally with `cargo run -p mcp`).
2. Obtain an MCP session id (the server returns `Mcp-Session-Id` on the first `POST /mcp`).

```bash
# get a new session id (one-liner; shell-specific parsing)
SESSION=$(curl -si -X POST http://localhost:3003/mcp | tr -d '\r' | awk '/Mcp-Session-Id:/ { print $2 }')

# call memory_retrieve using the session id and your API key
curl -s -X POST http://localhost:3003/mcp \
  -H "Mcp-Session-Id: $SESSION" \
  -H "Authorization: Bearer mops_..._<32b>" \
  -H "Content-Type: application/json" \
  -d '{"tool":"memory_retrieve","input":{"query":"auth decisions","limit":3}}'
```

If you receive a JSON response with a `memories` array, the MCP server is functioning for HTTP clients.

### B — Verify from Claude Code (in-chat / CLI)

1. Add the `.mcp.json` example above to the repo root and restart `claude` (or reload its config).
2. Start a `claude` session and ask it to list or call the MemoryOps tools. Example chat prompt:

```
System: You have an MCP server configured under the name "memoryops". List available tools for that server.
User: Call the tool `memory_retrieve` on "memoryops" with input { "query": "auth decisions", "limit": 2 } and show me the results.
```

If configured correctly, Claude Code will either show the available tools or execute the `memory_retrieve` tool and return the memory results. If no tools appear, restart `claude` and check `.mcp.json` placement.

## Using MemoryOps in Claude Code — practical prompts

- Retrieve prior context before edits:

```
User: Before I change the auth service, fetch recent memories about "api keys" and summarize key decisions.
```

- Store a post-refactor decision:

```
User: Record this decision as a memory: "Adopt workspace-scoped API keys for auth service" with tags ["decision","auth"].
```

- Incident post-mortem (timeline):

```
User: Use `memory_timeline` to reconstruct what the agent/project knew between 2026-04-10 and 2026-04-15 for the auth service.
```

- Contradictions workflow:

```
User: List unresolved contradictions with `memory_list_contradictions` and for each, propose a resolution; then call `memory_resolve_contradiction` with keep/discard decisions.
```

Claude can call tools directly when the server is configured; use structured prompts like the examples above and allow the assistant to emit tool calls.

## Recommended daily workflow

- Start session: `memory_retrieve` for relevant repo/workspace tags (e.g., `auth`, `ingestion`).
- During work: emit `memory_observe`/`memory_store` for decisions, planned changes, and important observations.
- End session: run a final `memory_store` summarizing the session's decisions.

## .gitignore / secrets

- Do NOT commit `.mcp.json` with a plain API key. Add `.mcp.json` to `.gitignore` or replace the key with `"${MEMORYOPS_API_KEY}"` and use environment substitution if Claude Code supports it.

## Troubleshooting

| Symptom | Likely cause / action |
|---|---|
| stdio process not starting | `cargo` not in PATH or the `command`/`args` are incorrect. Test `cargo run -p mcp` manually. Ensure the env block in `.mcp.json` contains required vars (MCP_TRANSPORT, DATABASE_URL, etc.). |
| HTTP connection refused | MCP not running on `:3003`. Start via `docker compose --profile mcp up -d` or `cargo run -p mcp`. Test with the HTTP smoke test above. |
| tools not appearing in Claude | Restart `claude` after changing `.mcp.json`. Confirm file is at the project root and valid JSON. Prefer project-scoped over global. |
| unexpected 401 from Claude | When using HTTP transport, ensure the `headers.Authorization` value is exactly `Bearer mops_...` and the key is valid for your workspace. |

## References

- MCP transport lifecycle: [mcp-transport.md](../mcp-transport.md)
- Local dev quickstart: [local-development.md](../local-development.md)
- MemoryOps API spec: [openapi.yaml](../openapi.yaml)

---
If you want, I can also add a sample `.mcp.json` to this repo root (gitignored) with placeholders. Would you like that?
