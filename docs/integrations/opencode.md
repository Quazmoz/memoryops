# OpenCode Integration

OpenCode can use MemoryOps through MCP when your OpenCode build supports MCP configuration. OpenCode configuration uses the top-level `mcp` object, with MCP server entries configured as `type: "remote"` for HTTP servers or `type: "local"` for command-started stdio servers.

Use the REST context export workflow as the fallback when MCP is unavailable or when you want a one-shot context file for a specific coding task.

## Prerequisites

- MemoryOps API is running on `http://localhost:8080` or another reachable URL.
- MemoryOps MCP server is running on `http://localhost:3003/mcp` when using remote/HTTP MCP.
- You have a MemoryOps workspace API key.
- OpenCode can read either a project `opencode.json` / `opencode.jsonc` file or a global config under `~/.config/opencode/opencode.json`.

Start the containerized MCP server:

```bash
docker compose up -d mcp
```

Set your MemoryOps API key in the shell that starts OpenCode:

```bash
export MEMORYOPS_API_KEY="YOUR_MEMORYOPS_API_KEY"
```

PowerShell:

```powershell
$env:MEMORYOPS_API_KEY = "YOUR_MEMORYOPS_API_KEY"
```

## Remote HTTP MCP configuration

Use this when MemoryOps MCP is already running as an HTTP service, including the Docker Compose `mcp` service.

Create or update `opencode.jsonc` in the project root:

```jsonc
{
  "$schema": "https://opencode.ai/config.json",
  "mcp": {
    "memoryops": {
      "type": "remote",
      "url": "http://localhost:3003/mcp",
      "enabled": true,
      "oauth": false,
      "headers": {
        "Authorization": "Bearer {env:MEMORYOPS_API_KEY}"
      },
      "timeout": 10000
    }
  }
}
```

Why this shape:

- `type: "remote"` is the OpenCode config type for HTTP MCP servers.
- `oauth: false` prevents OpenCode from trying an OAuth flow for a local API-key-protected server.
- `{env:MEMORYOPS_API_KEY}` keeps secrets out of the repository config.

Prompt OpenCode with an explicit tool hint when needed:

```text
Before editing, use memoryops to retrieve repo conventions, recent architectural decisions, and known failure modes.
```

## Local stdio MCP configuration

Use this when you want OpenCode to start the MemoryOps MCP server as a local process.

```jsonc
{
  "$schema": "https://opencode.ai/config.json",
  "mcp": {
    "memoryops": {
      "type": "local",
      "command": ["cargo", "run", "-p", "mcp"],
      "cwd": "/absolute/path/to/memoryops",
      "enabled": true,
      "environment": {
        "MEMORYOPS_API_URL": "http://localhost:8080",
        "MEMORYOPS_WORKSPACE_ID": "YOUR_WORKSPACE_ID",
        "MEMORYOPS_API_KEY": "{env:MEMORYOPS_API_KEY}",
        "MCP_TRANSPORT": "stdio"
      },
      "timeout": 10000
    }
  }
}
```

If you install or build a `memoryops-mcp` binary and put it on PATH, replace the command with:

```jsonc
"command": ["memoryops-mcp"]
```

## Recommended OpenCode operating rule

Add this to your OpenCode project instructions or `AGENTS.md`:

```text
Before project-specific code changes, query MemoryOps for repository conventions, recent architectural decisions, and known failure modes. Use returned memory as supporting context, not as a replacement for reading current files. After changes, store only durable decisions, root causes, and reusable implementation notes. Never store secrets, private reasoning, or transient task steps.
```

## Fallback: REST context export

When MCP is not available, export a context file and reference it in the OpenCode session:

```bash
node /path/to/memoryops/scripts/memoryops-client.js context \
  "What MemoryOps context matters for this OpenCode task?" \
  --client opencode \
  --repo auto \
  --token-budget 3000 \
  --out .memoryops/context.md \
  --prompt-out .memoryops/opencode-prompt.txt
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

If this works but OpenCode cannot connect, the issue is likely in the OpenCode config path, JSON shape, process environment, or MCP transport selection rather than MemoryOps itself.

## Debugging OpenCode MCP

Run:

```bash
opencode mcp list
opencode mcp debug memoryops
```

For remote HTTP MCP, check that `MEMORYOPS_API_KEY` exists in the same shell/session that starts OpenCode. For local stdio MCP, check that `cwd` points to the MemoryOps repository and that `cargo run -p mcp` works from that directory.
