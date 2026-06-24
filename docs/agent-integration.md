# External Agent Integration Guide

This guide explains how Claude Code, Gemini, OpenAI custom agents, Cursor, VS Code, and custom scripts can use a running MemoryOps instance.

Use placeholders in examples:

```bash
export MEMORYOPS_API_KEY="YOUR_MEMORYOPS_API_KEY"
export MEMORYOPS_WORKSPACE_ID="YOUR_WORKSPACE_ID"
export MEMORYOPS_API_URL="http://localhost:8080"
```

Do not commit API keys, `.mcp.json` files containing credentials, or downloaded local config files.

## Agent Library Model

MemoryOps stores reusable agent assets in the canonical, versioned Agent Library tables:

- `agent_resources`: current resource state.
- `agent_resource_versions`: immutable version snapshots.

Each resource has:

- `kind`: `skill`, `agent`, `prompt`, or `instruction`.
- `assistant`: `generic`, `openai`, `claude`, or `gemini`.
- `name`: stable lowercase identifier used in API paths.
- `title` and `description`: single-line UI and markdown summaries.
- `body`: editable markdown source.
- `content`: rendered/exportable markdown. If omitted, MemoryOps renders it from kind, title, description, and body.
- `metadata`: JSON object for source, default/custom status, tags, owners, or runtime hints.
- `version`: current version number. Every update and rollback creates a new version.

Skills are intentionally limited to `claude` and `gemini` targets because the legacy skill sync directories are `.claude/skills` and `.gemini/skills`. Agents, prompts, and instructions can target `generic`, `openai`, `claude`, or `gemini`.

## Canonical vs Legacy APIs

Prefer `/v1/agent-resources` for new integrations. It supports all four resource kinds, metadata, version listing, version reads, rollback, and canonical delete behavior.

The legacy `/v1/agent-skills` API remains available for existing Claude/Gemini skill sync workflows. It is backed by canonical `agent_resources` rows where `kind = skill`, and writes through the legacy API are mirrored into the versioned Agent Library.

## Lightweight REST Client

For simple repository-local automation, copy the helper client into another repo:

```bash
cp /path/to/memoryops/scripts/memoryops-client.js ./scripts/
chmod +x ./scripts/memoryops-client.js
```

Then configure the target repository:

```bash
export MEMORYOPS_API_KEY="YOUR_MEMORYOPS_API_KEY"
export MEMORYOPS_WORKSPACE_ID="YOUR_WORKSPACE_ID"
export MEMORYOPS_API_URL="http://localhost:8080"
```

Common commands:

```bash
node scripts/memoryops-client.js retrieve "Qdrant configuration decisions"
node scripts/memoryops-client.js store "Moved Qdrant gRPC clients to port 6334 for container networking" qdrant docker
node scripts/memoryops-client.js observe "API container saw connection timeout while resolving qdrant service" qdrant networking
node scripts/memoryops-client.js sync-skills
```

## Agent Resource API Examples

List all resources:

```bash
curl -s "$MEMORYOPS_API_URL/v1/agent-resources" \
  -H "X-API-Key: $MEMORYOPS_API_KEY" | jq
```

Create a prompt:

```bash
curl -s -X POST "$MEMORYOPS_API_URL/v1/agent-resources" \
  -H "X-API-Key: $MEMORYOPS_API_KEY" \
  -H "Content-Type: application/json" \
  -d '{
    "kind": "prompt",
    "assistant": "generic",
    "name": "release_brief",
    "title": "Release Brief Prompt",
    "description": "Drafts concise release notes from merged changes.",
    "body": "## Prompt\nSummarize the release impact, risks, and verification evidence.\n\n## Output\nReturn five bullets and one rollback note.",
    "metadata": { "default": false, "owner": "release" },
    "change_note": "Initial prompt"
  }' | jq
```

Create an instruction:

```bash
curl -s -X POST "$MEMORYOPS_API_URL/v1/agent-resources" \
  -H "X-API-Key: $MEMORYOPS_API_KEY" \
  -H "Content-Type: application/json" \
  -d '{
    "kind": "instruction",
    "assistant": "generic",
    "name": "no_secret_output",
    "title": "No Secret Output",
    "description": "Prevents agents from printing or storing sensitive values.",
    "body": "## Instruction\nNever print, store, or commit plaintext credentials. Replace examples with obvious placeholders.\n\n## Applies When\nAny task involves auth headers, tokens, keys, or webhook secrets."
  }' | jq
```

Create an agent profile:

```bash
curl -s -X POST "$MEMORYOPS_API_URL/v1/agent-resources" \
  -H "X-API-Key: $MEMORYOPS_API_KEY" \
  -H "Content-Type: application/json" \
  -d '{
    "kind": "agent",
    "assistant": "openai",
    "name": "production_reviewer",
    "title": "Production Code Review Agent",
    "description": "Reviews code for correctness, safety, migrations, and missing tests.",
    "body": "## Role\nAct as a risk-first production reviewer.\n\n## Operating Rules\nLead with bugs, regressions, data safety issues, and missing tests. Include file and line references."
  }' | jq
```

Download a Claude skill:

```bash
mkdir -p .claude/skills
curl -s "$MEMORYOPS_API_URL/v1/agent-resources/skill/claude/use_memoryops" \
  -H "X-API-Key: $MEMORYOPS_API_KEY" \
  | jq -r .content > .claude/skills/use_memoryops.md
```

Roll back a bad resource version:

```bash
curl -s -X POST "$MEMORYOPS_API_URL/v1/agent-resources/prompt/generic/release_brief/versions/1/rollback" \
  -H "X-API-Key: $MEMORYOPS_API_KEY" \
  -H "Content-Type: application/json" \
  -d '{"change_note": "Restore stable release brief"}' | jq
```

## Runtime Setup

### Claude Code

For skill files:

```bash
mkdir -p .claude/skills
curl -s "$MEMORYOPS_API_URL/v1/agent-resources/skill/claude/use_memoryops" \
  -H "X-API-Key: $MEMORYOPS_API_KEY" \
  | jq -r .content > .claude/skills/use_memoryops.md
```

For MCP, add project-local `.mcp.json` and keep it out of git:

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

### Gemini

Download Gemini-targeted skill files:

```bash
mkdir -p .gemini/skills
curl -s "$MEMORYOPS_API_URL/v1/agent-resources/skill/gemini/use_memoryops" \
  -H "X-API-Key: $MEMORYOPS_API_KEY" \
  | jq -r .content > .gemini/skills/use_memoryops.md
```

Use `agent-library/gemini/prompts`, `agent-library/gemini/agents`, and `agent-library/gemini/instructions` as portable export folders for non-skill resources.

### OpenAI And Generic Agents

OpenAI and generic agents should consume prompts, agent profiles, and reusable instructions from `/v1/agent-resources`. A common local folder convention is:

```text
agent-library/openai/agents/
agent-library/openai/prompts/
agent-library/openai/instructions/
agent-library/generic/agents/
agent-library/generic/prompts/
agent-library/generic/instructions/
```

Use the `content` field as the copy-ready markdown export. Keep `metadata` with the export when your runtime supports sidecar JSON.

### Cursor And VS Code

Use the HTTP MCP transport when the client supports it:

```json
{
  "contextProviders": [
    {
      "name": "mcp",
      "options": {
        "url": "http://localhost:3003/mcp",
        "headers": {
          "Authorization": "Bearer YOUR_MEMORYOPS_API_KEY"
        }
      }
    }
  ]
}
```

The VS Code extension can continue using the legacy Agent Skills sync command for Claude/Gemini skills while the Control Center manages the broader Agent Library.

## Recommended Agent Memory Rules

Add these rules to custom agents that cannot consume skill files directly:

```markdown
# MemoryOps Rules

1. Retrieve MemoryOps context before project-specific code changes, incident triage, migrations, or release decisions.
2. Use retrieved memories only when they match the current workspace, repository, service, and time horizon.
3. Treat conflicting memories as evidence to reconcile, not facts to silently choose between.
4. Store only durable outcomes: decisions, root causes, stable preferences, migration notes, and reusable workflow rules.
5. Use observations for raw logs or partial evidence.
6. Never store secrets, credentials, private reasoning, or transient task steps.
```

## Troubleshooting

- `401 unauthorized`: verify `X-API-Key` or `Authorization: Bearer YOUR_MEMORYOPS_API_KEY`.
- `400 validation_error`: check the resource name pattern `^[a-z][a-z0-9_-]{0,63}$`, body/content length, and metadata object shape.
- `409 conflict`: another resource already uses the same `(kind, assistant, name)` in this workspace.
- Missing skill after delete: default skills may be re-seeded when a workspace has no skill resources; create a custom replacement or keep at least one skill resource.
