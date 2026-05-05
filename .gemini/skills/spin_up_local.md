# Skill: Spin Up Local Development Environment

**Description:** Automatically spins up the entire MemoryOps stack (infrastructure, backend API, and containerized frontend), runs migrations, bootstraps a workspace, and injects the proper configuration.

## Trigger
Use this skill when the user asks to "spin up a local testing instance", "start the local dev environment", or "run the code locally".

## Execution Steps

Follow these steps precisely using your available terminal and file manipulation tools:

1. **Environment Setup**
   - Check if `.env` exists. If not, copy `.env.example` to `.env`.

2. **Start Infrastructure**
   - Run `docker compose up -d postgres redis qdrant` in the project root.
   - Run `docker ps` to verify that `memoryops-postgres`, `memoryops-redis`, and `memoryops-qdrant` are running and healthy.

3. **Database Migrations**
   - Apply the schema by running `sqlx migrate run`.
   - *(Note: `sqlx` typically reads `.env` automatically, but if on Windows PowerShell, ensure the variables are loaded).*

4. **Start API Server** (background process)
   - Start the backend API and all dependencies via Docker.
   - PowerShell:
     ```powershell
     docker compose up -d api
     ```
   - Use `curl.exe http://localhost:8080/health` to poll until it responds.

5. **Bootstrap Workspace & Key**
   - Execute a POST request to create a workspace:
     ```powershell
     $name = "Local Dev Workspace " + (Get-Date -Format "yyyyMMddHHmmss")
     $body = @{ name = $name } | ConvertTo-Json
     $workspace = Invoke-RestMethod -Uri "http://localhost:8080/v1/workspaces" -Method Post -ContentType "application/json" -Body $body
     $workspace_id = $workspace.workspace_id
     # Use the bootstrap key automatically created during workspace initialization
     $key = $workspace.api_key
     ```

6. **Start Frontend Container**
   - Ensure the frontend is built with the workspace ID and started.
   - PowerShell:
     ```powershell
     $env:VITE_MEMORYOPS_WORKSPACE_ID=$workspace_id
     docker compose up -d --build frontend
     ```
   - Verify it initializes and port `5173` is reachable.

7. **Seed Test Data** (optional)
   - Run the seed script if bash is available, or skip.
   - PowerShell: `if (Get-Command bash -ErrorAction SilentlyContinue) { $env:API_KEY=$key; bash scripts/seed.sh }`

8. **Handover to User**
   - Provide the **API Key** and **Workspace ID**.
   - URL: `http://localhost:5173/` (or `5174` if local fallback used).

