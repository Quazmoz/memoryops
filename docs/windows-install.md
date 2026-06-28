# Windows Installation Guide

This guide gets MemoryOps running on Windows for local testing and agent integrations such as AiderDesk, Aider, OpenCode, Claude Code, VS Code, and Open WebUI.

The recommended Windows path is Docker Desktop with the WSL 2 backend. Native Windows development can work, but Docker avoids most Postgres, Redis, Qdrant, OpenSSL, and Rust linker friction.

## Prerequisites

Install:

- Windows 11 or modern Windows 10 with virtualization enabled.
- Docker Desktop with the WSL 2 backend.
- Git for Windows.
- Node.js 20+.
- PowerShell 7+ recommended.
- Rust stable only if you plan to run crates outside Docker.

Confirm the basics:

```powershell
docker version
docker compose version
git --version
node --version
```

## Clone and configure

```powershell
git clone https://github.com/Quazmoz/memoryops.git
cd memoryops
Copy-Item .env.example .env
```

Edit `.env` and set:

```text
APP_SECRET_KEY=replace-with-a-long-random-string
WORKSPACE_CREATION_SECRET=replace-with-a-long-random-string
```

Generate a quick local random value:

```powershell
[guid]::NewGuid().ToString() + [guid]::NewGuid().ToString()
```

## Start MemoryOps

```powershell
docker compose build --no-cache api mcp frontend
docker compose up -d
docker compose ps
```

Expected local endpoints:

| Service | URL |
|---------|-----|
| API | `http://localhost:8080` |
| Frontend | `http://localhost:5173` |
| MCP | `http://localhost:3003/mcp` |

## Bootstrap a workspace

Load the workspace creation secret from `.env` into the current PowerShell session:

```powershell
Get-Content .env | ForEach-Object {
  if ($_ -match '^WORKSPACE_CREATION_SECRET=(.*)$') {
    $env:WORKSPACE_CREATION_SECRET = $matches[1].Trim()
  }
}
```

Run bootstrap:

```powershell
node scripts/bootstrap.mjs
```

Save the returned workspace id and workspace key. The workspace key is returned once.

## Seed demo data

```powershell
$env:WORKSPACE_ID = "YOUR_WORKSPACE_ID"
$env:API_KEY = "YOUR_MEMORYOPS_WORKSPACE_KEY"
node scripts/seed.mjs
```

Open the UI:

```powershell
Start-Process http://localhost:5173
```

## Configure agent credentials locally

For AiderDesk, Aider, OpenCode fallback mode, or CLI scripts, create a local-only config in the target repository:

```powershell
@'
{
  "api_url": "http://localhost:8080",
  "workspace_id": "YOUR_WORKSPACE_ID",
  "api_key": "YOUR_MEMORYOPS_WORKSPACE_KEY"
}
'@ | Set-Content .memoryops.local.json
```

Add local generated files to `.gitignore`:

```powershell
Add-Content .gitignore "`n.memoryops.local.json`n.memoryops/"
```

Export context for a non-MCP coding agent:

```powershell
New-Item -ItemType Directory -Force .memoryops | Out-Null
node ./scripts/memoryops-client.js context `
  "What repository context should the coding agent know before this task?" `
  --client aider `
  --repo auto `
  --token-budget 3000 `
  --out .memoryops/context.md `
  --prompt-out .memoryops/aider-prompt.txt
```

For terminal Aider, use the generated file as read-only context:

```powershell
aider --read .memoryops/context.md --load .memoryops/aider-prompt.txt <files-to-edit>
```

For AiderDesk, attach `.memoryops/context.md` as read-only or reference context when the UI supports it.

## OpenCode on Windows

For Docker-hosted MemoryOps, use the OpenCode remote MCP config from [docs/integrations/opencode.md](integrations/opencode.md). That guide has the canonical `opencode.jsonc` shape for OpenCode's `mcp` config object.

Use the same workspace key in the shell that starts OpenCode:

```powershell
$env:MEMORYOPS_API_KEY = "YOUR_MEMORYOPS_WORKSPACE_KEY"
```

## Troubleshooting

| Issue | Fix |
|-------|-----|
| Docker says virtualization is disabled | Enable virtualization in BIOS/UEFI and ensure WSL 2 is installed. |
| Ports already in use | Check `netstat -ano | findstr :8080`, `:5173`, `:3003`, `:5432`, `:6379`, or `:6334`, then stop the conflicting process or change compose ports. |
| Containers build but frontend is stale | Run `docker compose build --no-cache frontend` then `docker compose up -d --force-recreate frontend`. |
| API returns 401 | Verify the workspace key and confirm you are not using the workspace creation secret. |
| MCP client cannot connect | Confirm `docker compose ps mcp` is healthy. Browser GET requests to the MCP endpoint may not be meaningful. |
| Script cannot find credentials | Set `MEMORYOPS_API_KEY`, `MEMORYOPS_WORKSPACE_ID`, and `MEMORYOPS_API_URL`, or run it from a folder containing `.memoryops.local.json`. |
| `--repo auto` fails | Run the context command from a git repository with a GitHub origin, or pass `--repo owner/name` explicitly. |

## Reset local data

To restart containers without deleting data:

```powershell
docker compose down
docker compose up -d
```

To delete all local MemoryOps data and start over:

```powershell
docker compose down -v
```

This removes local Postgres, Redis, and Qdrant volumes.
