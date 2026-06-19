## Claude Code — MemoryOps integration

This guide explains how to configure Claude Code to use MemoryOps as an MCP server. It focuses on project-scoped configuration (`.mcp.json`) and both HTTP and stdio transports.

## Prerequisites

- Claude Code installed (`npm install -g @anthropic-ai/claude-code`) or via Homebrew
- MemoryOps stack running (postgres, redis, qdrant, API server on :8080)
- For HTTP transport: MCP server running with `MCP_TRANSPORT=http`
- For stdio transport: `cargo` in PATH and required env vars available to the spawned process
- API key from `POST /v1/workspaces`

## Configuration — Project-Scoped (Recommended)

Add a `.mcp.json` to the repo root. If it contains a real API key, add it to `.gitignore`.

Option A — HTTP transport (MCP server already running externally):

```json
{
  "mcpServers": {
    "memoryops": {
      "type": "http",
      "url": "http://localhost:3003/mcp",
      "headers": {
        "Authorization": "Bearer YOUR_MEMORYOPS_API_KEY"
      }
    }
  }
}
```

Warning: with HTTP transport, the MCP server must already be running. This transport is best when you want the server shared across multiple clients (Open WebUI + Claude Code simultaneously).

Option B — stdio transport (Claude Code spawns the process):

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

Note: `stdio` spawns `cargo run` on every Claude Code session start. Cold Rust builds take 2–5 minutes on first run. Consider warming the build cache or using a pre-built binary.

Option C — stdio with pre-built binary (best startup time):

```json
{
  "mcpServers": {
    "memoryops": {
      "type": "stdio",
      "command": "./target/debug/mcp",
      "args": [],
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

Build first:

```bash
cargo build -p mcp
```

## Configuration — Global (Optional)

For a user-global configuration put the same `mcpServers` block in `~/.claude.json`. This is convenient if you use MemoryOps across multiple projects.

## .gitignore Note

`.mcp.json` with a real API key should not be committed. Add to `.gitignore`:

```
.mcp.json
```

For team use, add `.mcp.json.example` with a `YOUR_MEMORYOPS_API_KEY` placeholder and document usage in the repo README.

## Verify the Connection

After editing `.mcp.json` restart Claude Code. To verify tools are loaded, issue the following in a Claude Code session:

/mcp

The server list should include `memoryops` and the toolset (all 11 tools).

Test a retrieval:

Use `memory_retrieve` with `query: "recent auth service changes"` and `limit: 5` to confirm end-to-end connectivity.

## Daily Workflow

Session start — retrieve context before coding:

Use `memory_retrieve` to fetch context about the retrieval crate, e.g. `limit: 8`.

After a refactor — store the decision:

Use `memory_store` to persist a decision:

```json
{
  "content": "Switched retrieval scoring weights: semantic_similarity raised to 0.40, recency lowered to 0.15 — tuned for long-lived semantic memories in engineering workspaces.",
  "agent_id": "claude-code",
  "tags": ["retrieval","scoring","config"],
  "importance": 0.8
}
```

Unstructured note — use observe for async classification:

Use `memory_observe` with:

```json
{
  "content": "Qdrant gRPC port 6334 must be used for the Rust client — HTTP port 6333 is for the REST dashboard only.",
  "tags": ["qdrant","infra"]
}
```

Incident post-mortem — time-travel query:

Use `memory_timeline`:

```json
{
  "query": "processor slow path jobs",
  "as_of": "2026-04-15T00:00:00Z",
  "limit": 10
}
```

Contradiction audit:

Use `memory_list_contradictions` to surface flagged conflicts. If contradictions exist resolve one with `memory_resolve_contradiction`:

```json
{ "contradiction_id": "...", "resolution": "keep_a" }
```

## Inline feedback — close the learning loop

Submit ratings from a previous retrieve inline in your next `memory_retrieve` call:

```json
{
  "query": "...",
  "feedback": {
    "query_id": "<trace-id from yesterday's retrieve>",
    "ratings": [
      { "memory_id": "<uuid>", "rating": 1 },
      { "memory_id": "<uuid>", "rating": -1 }
    ]
  }
}
```

## `memory_store` vs `memory_observe` — Which to Use

Same guidance as other integrations: `memory_store` for important, well-formed decisions you want immediately retrievable; `memory_observe` for raw notes that should be classified and scored by the async processor.

Rule: prefer `memory_store` for next-session retrieval importance; prefer `memory_observe` for dump-of-consciousness or noisy observations.

## Troubleshooting

| Symptom | Fix |
|---|---|
| `memoryops` not in `/mcp` list | Restart Claude Code after editing `.mcp.json`. Check JSON is valid. |
| stdio: `cargo: command not found` | `cargo` must be in the PATH Claude Code sees. On macOS, add to shell profile: `source "$HOME/.cargo/env"`. |
| stdio: process exits immediately | Missing env vars. Verify `DATABASE_URL`, `REDIS_URL`, `QDRANT_URL` are set in the `env` block. Check infra is up: `docker compose ps`. |
| http: connection refused | MCP server not running. Start the loopback-bound compose MCP service or run `MCP_TRANSPORT=http cargo run -p mcp`. |
| Tools visible but calls return 401 | `Authorization: Bearer` header value is wrong. Regenerate key: `POST /v1/workspaces/{id}/keys`. |
| Cold stdio start very slow | Build the binary first: `cargo build -p mcp`. Then use Option C (pre-built binary path) in `.mcp.json`. |
| `memories: []` always returned | No memories ingested. Run `API_KEY=... bash scripts/seed.sh` for sample data, or store a test memory directly. |

## References

- [mcp-transport.md](../mcp-transport.md)
- [local-development.md](../local-development.md)
- [bootstrap.md](../bootstrap.md)
- [openapi.yaml](../openapi.yaml)
