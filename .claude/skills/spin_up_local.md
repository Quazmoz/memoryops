# Skill: Spin Up Local Development Environment

**Description:** Spins up the entire MemoryOps stack (infrastructure, backend API, containerized frontend), runs migrations, bootstraps a workspace and API key, and optionally seeds test data.

## Trigger
Use this skill when the user asks to "spin up a local testing instance", "start the local dev environment", "run the code locally", or "spin up containers".

## Execution Steps

Follow these steps precisely using available terminal and file tools:

1. **Environment Setup**
   - Check if `.env` exists. If not, copy `.env.example` to `.env`.
   - Verify the file has valid values (DATABASE_URL, REDIS_URL, QDRANT_URL at minimum).

2. **Start Infrastructure**
   - Run: `docker compose up -d postgres redis qdrant`
   - Verify with `docker ps` that `memoryops-postgres`, `memoryops-redis`, and `memoryops-qdrant` are running and healthy.
   - Wait for health checks to pass (postgres typically takes 5-10s).

3. **Database Migrations**
   - On Windows, load env vars first then run migrations:
     ```powershell
     Get-Content .env | ForEach-Object { if ($_ -match '^\s*([^#]\w*)\s*=\s*(.*)') { [Environment]::SetEnvironmentVariable($matches[1], $matches[2]) } }; sqlx migrate run
     ```
   - On Linux/Mac: `source .env && sqlx migrate run` or just `sqlx migrate run` if .env is auto-loaded.
   - Confirm all migrations applied successfully.

4. **Start API Server** (background process)
   - On Windows PowerShell (background):
     ```powershell
     Start-Process powershell -ArgumentList '-NoExit', '-Command', 'Get-Content .env | ForEach-Object { if ($_ -match "^\s*([^#]\w*)\s*=\s*(.*)") { [Environment]::SetEnvironmentVariable($matches[1], $matches[2]) } }; cargo run -p api' -WindowStyle Minimized
     ```
   - Poll `http://localhost:8080/health` until it responds (API takes ~30-60s to compile on first run).

5. **Bootstrap Workspace & API Key**
   - Create workspace:
     ```
     curl.exe -s -X POST http://localhost:8080/v1/workspaces -H "Content-Type: application/json" -d "{\"name\": \"Local Dev Workspace\"}"
     ```
   - Extract `workspace_id` from the JSON response (use `| ConvertFrom-Json` in PowerShell or `jq` in bash).
   - Create API key for that workspace:
     ```
     curl.exe -s -X POST http://localhost:8080/v1/workspaces/<workspace_id>/keys -H "Content-Type: application/json" -d "{\"name\": \"Dev Key\"}"
     ```
   - Extract the plaintext `key` from the response.

6. **Start Frontend Container**
   - Set VITE_MEMORYOPS_WORKSPACE_ID to the workspace_id, then build and start:
     ```powershell
     $env:VITE_MEMORYOPS_WORKSPACE_ID="<workspace_id>"; docker compose up -d --build frontend
     ```
   - Confirm port 5173 is reachable.

7. **Seed Test Data** (optional but recommended)
   - Run the seed script with the API key:
     ```bash
     API_KEY=<key> bash scripts/seed.sh
     ```
   - On Windows: `$env:API_KEY="<key>"; bash scripts/seed.sh`
   - The script seeds episodic memories, semantic memories, pinned memories, and agent skills.

8. **Handover to User**
   - Provide the **API Key** explicitly.
   - Instruct the user to navigate to `http://localhost:5173/` and paste the API key into the "Or use existing" input on the setup screen.
