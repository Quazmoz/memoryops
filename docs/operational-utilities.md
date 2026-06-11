# MemoryOps Operational Utilities

MemoryOps includes dependency-free Node.js utilities for validating retrieval quality, auditing memory scope behavior, generating agent install profiles, importing memory, checking workspace health, and managing tool packs.

These tools are intentionally lightweight:

- no npm install step
- no database access
- no migrations
- no secrets written to disk by default
- compatible with local Docker setups and remote MemoryOps APIs

They resolve credentials from CLI flags, environment variables, or `.memoryops.local.json` in the current directory tree.

```json
{
  "api_url": "http://localhost:8080",
  "workspace_id": "<workspace-id>",
  "api_key": "<api-key>"
}
```

Do not commit `.memoryops.local.json`.

---

## 1. Retrieval evaluation harness

Use `scripts/memoryops-eval.mjs` to run golden-query suites against `/v1/retrieve`.

```bash
node scripts/memoryops-eval.mjs \
  --suite examples/evals/basic-memoryops.eval.json
```

Fail a manual or CI run if quality drops below a threshold:

```bash
node scripts/memoryops-eval.mjs \
  --suite examples/evals/basic-memoryops.eval.json \
  --fail-under 0.8
```

Print machine-readable output:

```bash
node scripts/memoryops-eval.mjs \
  --suite examples/evals/basic-memoryops.eval.json \
  --json
```

### Suite format

```json
{
  "name": "MemoryOps smoke eval",
  "defaults": {
    "limit": 5,
    "token_budget": 2048,
    "search_mode": "hybrid",
    "include_trace": true,
    "include_workspace_pool": true,
    "include_master_memory": true
  },
  "cases": [
    {
      "name": "Project purpose can be retrieved",
      "query": "What is MemoryOps used for?",
      "expected_contains": ["memory"],
      "must_not_contain": ["private key", "raw credential"]
    },
    {
      "name": "Retrieval stays inside token budget",
      "query": "Recent architecture decisions for retrieval",
      "max_total_tokens": 2048,
      "min_returned": 1
    }
  ]
}
```

Supported checks:

| Check | Purpose |
|---|---|
| `expected_memory_ids` | Requires specific memory IDs to be returned. Best for stable golden datasets. |
| `expected_contains` | Requires returned packed memory content to contain one or more fragments. |
| `must_not_contain` | Fails if returned packed memory content contains forbidden fragments. |
| `min_returned` | Requires at least this many memories. |
| `max_total_tokens` | Verifies the packed result stayed within a token ceiling. |

Recommended workflow:

1. Seed or ingest a known workspace.
2. Add 10-25 golden queries that represent common agent tasks.
3. Run the suite before retrieval, lifecycle, embedding, ranking, or token packing changes.
4. Store JSON output from major releases to compare quality trends over time.

---

## 2. Scope-aware retrieval audit

Use `scripts/memoryops-scope-audit.mjs` to inspect what an agent would receive under a specific scope.

```bash
node scripts/memoryops-scope-audit.mjs \
  "How should tool secrets be handled?" \
  --agent-id vscode \
  --repo Quazmoz/memoryops \
  --include-workspace-pool \
  --include-master-memory
```

Example point-in-time audit:

```bash
node scripts/memoryops-scope-audit.mjs \
  "auth service decisions" \
  --user-id quinn \
  --agent-id claude-code \
  --repo Quazmoz/memoryops \
  --as-of 2026-04-15T00:00:00Z
```

Useful flags:

| Flag | Purpose |
|---|---|
| `--agent-id` | Simulate retrieval for a specific agent. |
| `--user-id` | Simulate retrieval for a specific user. |
| `--repo` | Simulate repo-scoped retrieval. |
| `--source-ref` | Filter by source reference when backend support is available. |
| `--include-workspace-pool` | Include workspace-published semantic memory. |
| `--include-master-memory` | Include master/global memory. |
| `--memory-types episodic,semantic` | Limit retrieved memory types. |
| `--tags tag1,tag2` | Limit by tags. |
| `--json` | Print machine-readable audit output. |

The audit reports:

- request scope
- included memories
- inferred scope class per memory
- token counts and score fields when returned
- trace exclusions when backend trace candidates include exclusion details
- warnings for suspicious scope behavior

Use this before connecting a new agent profile so you can verify that private, workspace, and master memory are being inherited exactly as expected.

---

## 3. Agent install profile generator

Use `scripts/memoryops-agent-profile.mjs` to generate copy/paste profiles for agents and editor integrations.

```bash
node scripts/memoryops-agent-profile.mjs \
  --target vscode \
  --agent-id vscode \
  --repo Quazmoz/memoryops \
  --include-workspace-pool
```

Generate every supported target into `.memoryops/profiles`:

```bash
node scripts/memoryops-agent-profile.mjs \
  --target all \
  --repo Quazmoz/memoryops \
  --include-workspace-pool \
  --include-master-memory \
  --write
```

Supported targets:

- `vscode`
- `claude-code`
- `cursor`
- `openwebui`
- `gemini`
- `generic`
- `all`

Generated profiles include:

- API URL
- MCP URL
- workspace ID
- agent ID
- optional user ID
- optional repo scope
- retrieval defaults
- memory write policy
- target-specific setup notes

API keys are not embedded. Store credentials in the VS Code SecretStorage command, environment variables, MCP runtime secret config, or the agent runtime's secret manager.

---

## 4. Memory importer

Use `scripts/memoryops-import.mjs` to bootstrap a workspace from Markdown, text, JSON, or JSONL.

```bash
node scripts/memoryops-import.mjs \
  --path docs \
  --tags docs,bootstrap
```

Dry-run before writing:

```bash
node scripts/memoryops-import.mjs \
  --path README.md \
  --dry-run \
  --json
```

Import structured JSONL directly as memory units instead of observations:

```bash
node scripts/memoryops-import.mjs \
  --path exports/memories.jsonl \
  --format jsonl \
  --mode memory \
  --tags migrated
```

Supported input:

| Format | Behavior |
|---|---|
| Markdown/text file | Imports the file as one or more chunks. |
| Markdown/text directory | Recursively imports `.md`, `.mdx`, and `.txt` files. |
| JSON array | Imports each object with a `content` field. |
| JSONL/NDJSON | Imports one object per line with a `content` field. |

Common fields for JSON/JSONL items:

```json
{
  "content": "Durable memory text",
  "tags": ["runbook", "imported"],
  "source_ref": "docs/runbook.md",
  "agent_id": "docs-importer",
  "user_id": "optional-user",
  "repo": "owner/repo",
  "memory_type": "episodic",
  "importance_score": 0.7,
  "metadata": {
    "source": "legacy-export"
  }
}
```

Default mode is `observation`, which sends content through `/v1/ingest/observation`. Use `--mode memory` only when you intentionally want direct memory creation through `/v1/memory`.

---

## 5. Workspace health report

Use `scripts/memoryops-health-report.mjs` to generate an operational health score for a workspace.

```bash
node scripts/memoryops-health-report.mjs
```

Fail a release gate if the health score drops:

```bash
node scripts/memoryops-health-report.mjs \
  --fail-under 80
```

Machine-readable output:

```bash
node scripts/memoryops-health-report.mjs \
  --json
```

The report checks:

- `/health/ready`
- `/health/system`
- workspace config
- workspace stats
- stats history
- integrations
- DLQ jobs
- contradiction count
- tags
- retrieval smoke test

The score is intentionally simple: critical findings carry the largest penalty, warnings carry medium penalty, and info findings carry small penalty. Treat it as an operator-facing readiness indicator, not a formal SLO calculation.

---

## 6. Tool-pack utility

Use `scripts/memoryops-tool-pack.mjs` to validate, import, export, and list MemoryOps tools as installable packs.

Validate a pack:

```bash
node scripts/memoryops-tool-pack.mjs validate \
  --file examples/tool-packs/http-smoke.toolpack.json
```

Import a pack:

```bash
node scripts/memoryops-tool-pack.mjs import \
  --file examples/tool-packs/http-smoke.toolpack.json \
  --overwrite
```

Export workspace tools:

```bash
node scripts/memoryops-tool-pack.mjs export \
  --out .memoryops/tool-packs/workspace-tools.json
```

List registered tools:

```bash
node scripts/memoryops-tool-pack.mjs list
```

Tool pack shape:

```json
{
  "name": "memoryops-devops-pack",
  "version": "0.1.0",
  "tools": [
    {
      "name": "example_tool",
      "description": "Short explanation of what this tool does.",
      "endpoint_url": "https://example.com/api/tool",
      "http_method": "POST",
      "input_schema": { "type": "object", "properties": {} },
      "output_schema": { "type": "object" },
      "scope_visibility": "workspace",
      "enabled": true
    }
  ]
}
```

Secrets are intentionally omitted from exported packs. If a private pack needs an `auth_secret`, keep it out of version control and inject it through a secure workflow.

---

## Practical release gate

Before merging retrieval-related changes, run:

```bash
node scripts/memoryops-eval.mjs \
  --suite examples/evals/basic-memoryops.eval.json \
  --fail-under 0.8

node scripts/memoryops-scope-audit.mjs \
  "How should tool secrets be handled?" \
  --agent-id vscode \
  --repo Quazmoz/memoryops \
  --include-workspace-pool \
  --include-master-memory
```

Before connecting a new coding agent, run:

```bash
node scripts/memoryops-agent-profile.mjs \
  --target <target> \
  --agent-id <agent-id> \
  --repo <owner/name> \
  --write
```

Before importing external project knowledge, run:

```bash
node scripts/memoryops-import.mjs \
  --path <docs-or-export> \
  --dry-run
```

Before publishing a tool pack, run:

```bash
node scripts/memoryops-tool-pack.mjs validate \
  --file <tool-pack.json>
```
