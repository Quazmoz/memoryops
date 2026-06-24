# Connecting VS Code (GitHub Copilot / Continue.dev) to MemoryOps

## GitHub Copilot (VS Code with MCP Extension)

Prerequisites:

- VS Code 1.99 or newer.
- GitHub Copilot extension installed.
- MemoryOps MCP server running with HTTP transport.
- A MemoryOps API key from `POST /v1/workspaces`.

Add MemoryOps to `.vscode/mcp.json` for workspace-scoped configuration, or to your global VS Code MCP config.

On macOS, the global path is:

```text
~/Library/Application Support/Code/User/mcp.json
```

Use this server definition:

```json
{
  "servers": {
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

Verify the connection from Copilot chat:

```text
@memoryops memory_retrieve query="recent auth decisions"
```

Expected response shape:

```json
{
  "memories": [
    {
      "id": "019...",
      "content": "...",
      "memory_type": "episodic",
      "tags": [],
      "score": 0.82,
      "importance_score": 0.6,
      "created_at": "2026-04-30T12:00:00Z",
      "source": "memoryops"
    }
  ],
  "skills": []
}
```

For local development, `stdio` is also supported:

```json
{
  "servers": {
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

## Continue.dev

Prerequisites:

- Continue.dev extension installed.
- MemoryOps MCP server running with HTTP transport.
- A MemoryOps API key from `POST /v1/workspaces`.

Add MemoryOps under `mcpServers` in `~/.continue/config.json`:

```json
{
  "mcpServers": [
    {
      "name": "MemoryOps",
      "transport": {
        "type": "http",
        "url": "http://localhost:3003/mcp",
        "requestInit": {
          "headers": {
            "Authorization": "Bearer YOUR_MEMORYOPS_API_KEY"
          }
        }
      }
    }
  ]
}
```

Use a prompt that stores and retrieves memory in one pass:

```text
Use MemoryOps to store this memory: "The API service uses workspace-scoped API keys with the mops_ prefix." Then retrieve memories for the query "workspace API key format" and summarize the result.
```

The round trip should call `memory_store` first, then `memory_retrieve` with the query.
