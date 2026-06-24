---
name: memoryops-populate-repo
description: Populate the current repository with local MemoryOps agent setup files, skills, prompts, instructions, MCP configs, and profiles. Use when an agent is asked to connect any repo to MemoryOps, install MemoryOps agent assets, bootstrap MemoryOps usage outside the MemoryOps source repo, sync agent resources from a running MemoryOps server, or ask the user for the server DNS/IP, workspace ID, API key, MCP URL, agent ID, and target clients needed for MemoryOps integration.
---

# MemoryOps Repo Population

Configure the current repository so agents can retrieve, store, observe, and sync context through a running MemoryOps instance.

## Required Questions

Ask the user for any missing value before writing config:

- MemoryOps API URL, including protocol and port. Default to `http://localhost:8080`.
- MemoryOps MCP URL, including `/mcp` when using HTTP transport. Default to the API host on port `3003`, for example `http://localhost:3003/mcp`.
- Workspace ID.
- API key. Explain that it is optional for generating templates, but required to verify connectivity or sync server-managed resources.
- Whether to store the API key in repo-local `.memoryops.local.json`. Default to no; only do this with explicit consent because the file is plaintext even when gitignored.
- Agent ID. Default to the repository folder name normalized to lowercase hyphen-case.
- Optional user ID and repository scope, such as `owner/repo`.
- Target clients: `claude-code`, `vscode`, `cursor`, `gemini`, `openai`, `generic`, or `all`.
- Whether to include workspace-published memory and master/global memory in default retrieval profiles.
- Whether to create local MCP config files such as `.mcp.json` and `.vscode/mcp.json`.
- Whether to embed the API key in generated MCP config files. Default to no; use `YOUR_MEMORYOPS_API_KEY` placeholders unless the user explicitly approves plaintext local config.

Do not invent credentials. Do not print API keys in logs or summaries. Do not commit API keys, `.memoryops.local.json`, `.memoryops/`, `.mcp.json`, `.vscode/mcp.json`, or other secret-bearing local config.

## Fast Path

If Node.js is available and this skill folder includes `scripts/setup-memoryops-repo.mjs`, run:

```bash
node skills/memoryops-populate-repo/scripts/setup-memoryops-repo.mjs
```

Run it from the target repository root. Let it ask the user the required questions and create files. Afterward, summarize what was written and call out any manual steps, such as restarting the agent client or starting the MemoryOps MCP server.

## Manual Path

When the helper script is unavailable, create the same repo-local assets manually.

1. Add or update `.gitignore` with local MemoryOps secrets and private generated config:

```gitignore
.memoryops.local.json
.memoryops/
.mcp.json
.vscode/mcp.json
mcp.json
mcp.*.json
```

2. Create `.memoryops.local.json` only with the user's consent to store local connection details:

```json
{
  "api_url": "http://memoryops.example.internal:8080",
  "workspace_id": "YOUR_WORKSPACE_ID",
  "mcp_url": "http://memoryops.example.internal:3003/mcp",
  "agent_id": "coding-agent",
  "repo": "owner/repo"
}
```

Include `"api_key": "..."` only when the user explicitly approves storing it locally. Prefer `MEMORYOPS_API_KEY` or the client secret store for routine use.

3. Create `.memoryops/profiles/<target>.memoryops.md` for each requested target with:

- Connection fields: API URL, MCP URL, workspace ID, agent ID, optional user ID, optional repo.
- Retrieval defaults: `token_budget: 4096`, `search_mode: hybrid`, `include_trace: true`, requested inheritance flags.
- Memory rules: retrieve before non-trivial project work, store durable decisions, observe raw evidence, never store secrets, surface conflicts.

4. For Claude Code, create `.mcp.json` when requested. Use a placeholder token unless the user explicitly approves embedding the real key:

```json
{
  "mcpServers": {
    "memoryops": {
      "type": "http",
      "url": "http://memoryops.example.internal:3003/mcp",
      "headers": {
        "Authorization": "Bearer YOUR_MEMORYOPS_API_KEY"
      }
    }
  }
}
```

5. For VS Code or GitHub Copilot MCP, create `.vscode/mcp.json` when requested. Use a placeholder token unless the user explicitly approves embedding the real key:

```json
{
  "servers": {
    "memoryops": {
      "type": "http",
      "url": "http://memoryops.example.internal:3003/mcp",
      "headers": {
        "Authorization": "Bearer YOUR_MEMORYOPS_API_KEY"
      }
    }
  }
}
```

6. For Claude and Gemini skill-capable clients, create local fallback skills:

- `.claude/skills/use_memoryops.md`
- `.gemini/skills/use_memoryops.md`

The skill body must instruct the agent to retrieve MemoryOps context before substantial work, use MCP tools when available, use REST fallback when MCP is unavailable, store only durable decisions and reusable project facts, use observations for raw evidence, and never store secrets.

7. For OpenAI and generic agents, create:

- `agent-library/openai/instructions/memoryops-rules.md`
- `agent-library/generic/instructions/memoryops-rules.md`
- `agent-library/generic/agents/memoryops-coding-agent.md`

## Server Sync

If the user provides an API key and wants server-managed assets, prefer syncing from the canonical Agent Library:

- `GET /v1/agent-resources`
- `GET /v1/agent-resources/{kind}/{assistant}/{name}`

Write returned `content` to:

- Skills: `.<assistant>/skills/<name>.md` for `assistant` values `claude` or `gemini`.
- Prompts: `agent-library/<assistant>/prompts/<name>.md`.
- Agents: `agent-library/<assistant>/agents/<name>.md`.
- Instructions: `agent-library/<assistant>/instructions/<name>.md`.

If sync fails, keep the fallback local assets and tell the user which endpoint failed.

## Verification

Verify the setup when credentials are available:

- `GET <api_url>/health/ready` should succeed for API health.
- MCP clients should list a `memoryops` server after restart.
- A small retrieval query should return either memories or an empty, authenticated response, not `401`.

When verification cannot run because credentials or network access are unavailable, state that clearly and leave the generated files in place.
