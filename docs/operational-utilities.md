# MemoryOps Operational Utilities

MemoryOps now includes a small set of dependency-free Node.js utilities for validating retrieval quality, auditing memory scope behavior, and generating agent install profiles.

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
