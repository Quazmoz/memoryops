# AiderDesk / Aider Integration

Aider and AiderDesk can use MemoryOps today through the REST client workflow. This is the recommended path when the client does not expose a native MCP server configuration surface.

The workflow is:

1. Run MemoryOps locally or on a reachable server.
2. Export a small, token-packed context file from MemoryOps.
3. Add that file to the Aider/AiderDesk session as read-only context.
4. Store durable implementation decisions back into MemoryOps after the coding session.

## Prerequisites

- MemoryOps API is running.
- A workspace has been bootstrapped.
- You have a workspace API key.
- Node.js 20+ is available in the repository where you run the helper script.

Set credentials in your shell:

```bash
export MEMORYOPS_API_URL="http://localhost:8080"
export MEMORYOPS_WORKSPACE_ID="YOUR_WORKSPACE_ID"
export MEMORYOPS_API_KEY="YOUR_MEMORYOPS_API_KEY"
```

PowerShell equivalent:

```powershell
$env:MEMORYOPS_API_URL = "http://localhost:8080"
$env:MEMORYOPS_WORKSPACE_ID = "YOUR_WORKSPACE_ID"
$env:MEMORYOPS_API_KEY = "YOUR_MEMORYOPS_API_KEY"
```

Do not commit API keys or `.memoryops.local.json`.

## Export context for Aider

From the target repository, run:

```bash
mkdir -p .memoryops
node /path/to/memoryops/scripts/memoryops-client.js context \
  "What project conventions, architectural decisions, and recent bugs matter for this change?" \
  --repo Quazmoz/memoryops \
  --agent-id aider \
  --token-budget 3000 \
  --out .memoryops/context.md
```

PowerShell:

```powershell
New-Item -ItemType Directory -Force .memoryops | Out-Null
node C:\path\to\memoryops\scripts\memoryops-client.js context `
  "What project conventions, architectural decisions, and recent bugs matter for this change?" `
  --repo Quazmoz/memoryops `
  --agent-id aider `
  --token-budget 3000 `
  --out .memoryops/context.md
```

Then provide `.memoryops/context.md` to Aider/AiderDesk as read-only context. If your AiderDesk build has a file attachment or read-only file control, attach the file there. If you are using Aider from the terminal, add the context file as a read-only reference using your installed Aider version's read-only file option.

## Recommended Aider session prompt

```text
Use .memoryops/context.md as retrieved memory context. Treat repository files as the current source of truth when they conflict with memory. Make the requested code changes with minimal blast radius. After finishing, summarize only durable decisions, migrations, conventions, or root causes that should be stored back into MemoryOps. Do not store secrets or transient steps.
```

## Store durable outcomes after the session

After Aider finishes and you have reviewed the diff, store the durable result:

```bash
node /path/to/memoryops/scripts/memoryops-client.js store \
  "Aider change summary: <durable decision or implementation note>" \
  aider repo-memory implementation-note
```

For raw observations that still need processing, use `observe` instead:

```bash
node /path/to/memoryops/scripts/memoryops-client.js observe \
  "Aider observed repeated failures around <topic>; needs follow-up validation" \
  aider observation
```

## Project-local credentials option

For local-only convenience, place this in `.memoryops.local.json` at the repository root and add it to `.gitignore`:

```json
{
  "api_url": "http://localhost:8080",
  "workspace_id": "YOUR_WORKSPACE_ID",
  "api_key": "YOUR_MEMORYOPS_API_KEY"
}
```

The helper script searches upward from the current working directory, so subfolders can share the same local config.

## Security notes

- Keep `.memoryops/` and `.memoryops.local.json` out of git.
- Regenerate workspace API keys if they are pasted into an agent transcript or committed by mistake.
- Store stable engineering facts, not raw private chats, credentials, or short-lived task scratch work.
- Refresh `.memoryops/context.md` per task; do not treat old context files as canonical.
