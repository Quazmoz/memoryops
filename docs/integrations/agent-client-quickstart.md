# Agent Client Quickstart

Use this when you want to connect a coding client to MemoryOps quickly.

## Choose the integration path

| Client | Preferred path |
|--------|----------------|
| OpenCode | Native OpenCode `mcp` config. See `docs/integrations/opencode.md`. |
| Aider / AiderDesk | Export `.memoryops/context.md` and add it as read-only context. See `docs/integrations/aider-desk.md`. |
| Claude Code | MCP config plus Claude skill file. See `docs/integrations/claude-code.md`. |
| Open WebUI | MCP or OpenAPI-style tool wiring. See `docs/integrations/openwebui.md`. |
| Any other coding agent | Export `.memoryops/context.md` and pass it as reference context. |

## Export one-shot context

Run this from the repository you are editing:

```bash
mkdir -p .memoryops
node /path/to/memoryops/scripts/memoryops-client.js context \
  "What repo conventions, recent decisions, bugs, and gotchas matter for this task?" \
  --client aider \
  --repo auto \
  --token-budget 3000 \
  --out .memoryops/context.md \
  --prompt-out .memoryops/aider-prompt.txt
```

`--repo auto` reads `git remote get-url origin` and scopes retrieval to the current GitHub `owner/name` repo. Use `--repo owner/name` if auto-detection fails.

## Aider

```bash
aider --read .memoryops/context.md --load .memoryops/aider-prompt.txt <files-to-edit>
```

Inside a running Aider session:

```text
/read-only .memoryops/context.md
```

## OpenCode fallback without MCP

```bash
node /path/to/memoryops/scripts/memoryops-client.js context \
  "What MemoryOps context matters for this OpenCode task?" \
  --client opencode \
  --repo auto \
  --token-budget 3000 \
  --out .memoryops/context.md \
  --prompt-out .memoryops/opencode-prompt.txt
```

Then ask OpenCode to use `.memoryops/context.md` as retrieved MemoryOps context.

## Store durable outcomes

After reviewing the diff, store only stable facts that should survive the session:

```bash
node /path/to/memoryops/scripts/memoryops-client.js store \
  "Implemented <durable decision or convention>; validation: <evidence>." \
  agent-session implementation-note
```

Use `observe` instead of `store` for raw evidence that still needs classification.

## Local ignore rules

Keep generated context and local credentials out of git:

```gitignore
.memoryops.local.json
.memoryops/
```
