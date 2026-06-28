# AiderDesk / Aider Integration

Aider and AiderDesk can use MemoryOps today through a REST context-export workflow. This is the recommended path when the client does not expose a native MCP configuration surface.

The workflow is:

1. Run MemoryOps locally or on a reachable server.
2. Export a small, token-packed context file from MemoryOps.
3. Add that file to Aider/AiderDesk as read-only context.
4. Store durable implementation decisions back into MemoryOps after the coding session.

## Prerequisites

- MemoryOps API is running.
- A workspace has been bootstrapped.
- You have a workspace API key.
- Node.js 20+ is available in the repository where you run the helper script.
- Terminal Aider users can pass read-only files with `--read` at startup or `/read-only` inside a running session.

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

Do not commit API keys, `.memoryops.local.json`, or generated `.memoryops/` context files.

## Export context for Aider

From the target repository, run:

```bash
mkdir -p .memoryops
node /path/to/memoryops/scripts/memoryops-client.js context \
  "What project conventions, architectural decisions, and recent bugs matter for this change?" \
  --client aider \
  --repo auto \
  --token-budget 3000 \
  --out .memoryops/context.md \
  --prompt-out .memoryops/aider-prompt.txt
```

PowerShell:

```powershell
New-Item -ItemType Directory -Force .memoryops | Out-Null
node ./scripts/memoryops-client.js context `
  "What project conventions, architectural decisions, and recent bugs matter for this change?" `
  --client aider `
  --repo auto `
  --token-budget 3000 `
  --out .memoryops/context.md `
  --prompt-out .memoryops/aider-prompt.txt
```

`--repo auto` reads `git remote get-url origin` and sends an `owner/name` repo scope when the remote is GitHub. Use `--repo Quazmoz/memoryops` instead when auto-detection is not available.

## Use the context in terminal Aider

Start Aider with the context file as read-only:

```bash
aider --read .memoryops/context.md <files-to-edit>
```

Or add it inside an existing Aider session:

```text
/read-only .memoryops/context.md
```

Then paste the contents of `.memoryops/aider-prompt.txt` or use this prompt:

```text
Use .memoryops/context.md as retrieved memory context. Treat repository files as the current source of truth when they conflict with memory. Make the requested code changes with minimal blast radius. After finishing, summarize only durable decisions, migrations, conventions, or root causes that should be stored back into MemoryOps. Do not store secrets or transient steps.
```

You can also preload the companion prompt file if your Aider version supports loading command files:

```bash
aider --read .memoryops/context.md --load .memoryops/aider-prompt.txt <files-to-edit>
```

## Use the context in AiderDesk

AiderDesk UI capabilities vary by version. Prefer the safest available option in this order:

1. Add `.memoryops/context.md` through a read-only or reference-file control.
2. Attach `.memoryops/context.md` as context and explicitly instruct AiderDesk not to edit it.
3. Paste the compact context into the first message only if the UI cannot attach files.

Do not add `.memoryops/context.md` as an editable target file.

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

## Recommended `.gitignore`

```gitignore
.memoryops.local.json
.memoryops/
```

## Security notes

- Keep `.memoryops/` and `.memoryops.local.json` out of git.
- Regenerate workspace API keys if they are pasted into an agent transcript or committed by mistake.
- Store stable engineering facts, not raw private chats, credentials, or short-lived task scratch work.
- Refresh `.memoryops/context.md` per task; do not treat old context files as canonical.
