# OpenCode Integration

OpenCode can use MemoryOps through the MCP server when your OpenCode build supports MCP configuration. Use the REST context export workflow as the fallback for builds or environments where MCP is unavailable.

## Prerequisites

- MemoryOps API is running on `http://localhost:8080` or another reachable URL.
- MemoryOps MCP server is running on `http://localhost:3003/mcp` when using HTTP transport.
- You have a MemoryOps workspace API key.

Start the containerized MCP server:

```bash
docker compose up -d mcp
```

Or run it locally with stdio transport:

```bash
MEMORYOPS_API_URL=http://localhost:8080 \
MEMORYOPS_API_KEY=YOUR_MEMORYOPS_API_KEY \
MEMORYOPS_WORKSPACE_ID=YOUR_WORKSPACE_ID \
MCP_TRANSPORT=stdio \
cargo run -p mcp
```

## HTTP MCP configuration

Use HTTP MCP when OpenCode accepts remote MCP server definitions:

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

Keep this config local. Do not commit API keys.

## Stdio MCP configuration

Use stdio MCP when OpenCode starts MCP servers as local commands:

```json
{
  "mcpServers": {
    "memoryops": {
      "command": "memoryops-mcp",
      "args": [],
      "env": {
        "MEMORYOPS_API_URL": "http://localhost:8080",
        "MEMORYOPS_WORKSPACE_ID": "YOUR_WORKSPACE_ID",
        "MEMORYOPS_API_KEY": "YOUR_MEMORYOPS_API_KEY",
        "MCP_TRANSPORT": "stdio"
      }
    }
  }
}
```

If `memoryops-mcp` is not on PATH, replace the command with `cargo` and use args similar to:

```json
{
  "command": "cargo",
  "args": ["run", "-p", "mcp"]
}
```

Run that from the MemoryOps repository root.

## Recommended OpenCode operating rule

Add this to your OpenCode project instructions:

```text
Before project-specific code changes, query MemoryOps for repository conventions, recent architectural decisions, and known failure modes. Use returned memory as supporting context, not as a replacement for reading current files. After changes, store only durable decisions, root causes, and reusable implementation notes. Never store secrets, private reasoning, or transient task steps.
```

## Fallback: REST context export

When MCP is not available, export a context file and attach or reference it in the OpenCode session:

```bash
node /path/to/memoryops/scripts/memoryops-client.js context \
  "What MemoryOps context matters for this OpenCode task?" \
  --agent-id opencode \
  --repo Quazmoz/memoryops \
  --token-budget 3000 \
  --out .memoryops/context.md
```

Then tell OpenCode:

```text
Use .memoryops/context.md as retrieved MemoryOps context. Treat current repository files as the source of truth when they conflict with memory.
```

## Smoke test

Use the API directly to confirm credentials before debugging OpenCode config:

```bash
curl -s http://localhost:8080/v1/retrieve \
  -H "X-API-Key: YOUR_MEMORYOPS_API_KEY" \
  -H "Content-Type: application/json" \
  -d '{
    "workspace_id": "YOUR_WORKSPACE_ID",
    "query": "MemoryOps integration smoke test",
    "token_budget": 1000,
    "agent_id": "opencode"
  }' | jq
```

If this works but OpenCode cannot connect, the issue is likely in the MCP client config path, JSON shape, or process environment rather than MemoryOps itself.
