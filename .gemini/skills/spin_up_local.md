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
   - Start the backend API asynchronously.
   - On Windows PowerShell (background):
     ```powershell
     Start-Process powershell -ArgumentList '-NoExit', '-Command', 'Get-Content .env | ForEach-Object { if ($_ -match "^\s*([^#]\w*)\s*=\s*(.*)") { [Environment]::SetEnvironmentVariable($matches[1], $matches[2]) } }; cargo run -p api' -WindowStyle Minimized
     ```
   - Use `curl.exe http://localhost:8080/health` to poll until it responds (API takes ~30-60s to compile on first run).

5. **Bootstrap Workspace & Key**
   - Execute a POST request to create a workspace:
     ```powershell
     $workspaceResponse = curl.exe -s -X POST http://localhost:8080/v1/workspaces -H "Content-Type: application/json" -d '{\"name\": \"Local Dev Workspace\"}' | ConvertFrom-Json
     $workspace_id = $workspaceResponse.id
     ```
   - Execute a POST request to generate the API key for that workspace:
     ```powershell
     $keyResponse = curl.exe -s -X POST http://localhost:8080/v1/workspaces/$workspace_id/keys -H "Content-Type: application/json" -d '{\"name\": \"Dev Key\"}' | ConvertFrom-Json
     $key = $keyResponse.key
     ```

6. **Start Frontend Container**
   - Use the newly generated `workspace_id` to build and start the frontend container.
   - PowerShell command: `$env:VITE_MEMORYOPS_WORKSPACE_ID=\"$workspace_id\"; docker compose up -d --build frontend`
   - Verify it initializes and port `5173` is reachable.

7. **Seed Test Data** (optional but recommended)
   - Run the seed script with the API key:
     ```powershell
     $env:API_KEY=\"$key\"; bash scripts/seed.sh
     ```
   - The script seeds episodic memories, semantic memories, pinned memories, and agent skills.

8. **Handover to User**
   - Inform the user that the environment is successfully spun up.
   - Provide them with the **API Key** explicitly in your response.
   - Instruct them to navigate to `http://localhost:5173/` and paste the API key into the "Or use existing" input field on the setup screen to enter the app.

