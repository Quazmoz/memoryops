# Skill: Spin Up Local Development Environment

**Description:** Automatically spins up the entire MemoryOps stack (infrastructure, backend API, and frontend), runs migrations, bootstraps a workspace, and injects the proper configuration.

## Trigger
Use this skill when the user asks to "spin up a local testing instance", "start the local dev environment", or "run the code locally".

## Execution Steps

Follow these steps precisely using your available terminal and file manipulation tools:

1. **Environment Setup**
   - Check if `.env` exists. If not, copy `.env.example` to `.env`.
   - Check if `frontend/.env.local` exists. If not, copy `frontend/.env.example` to `frontend/.env.local`.

2. **Start Infrastructure**
   - Run `docker compose up -d` in the project root.
   - Run `docker ps` to verify that `memoryops-postgres`, `memoryops-redis`, and `memoryops-qdrant` are running and healthy.

3. **Database Migrations**
   - Apply the schema by running `sqlx migrate run`.
   - *(Note: `sqlx` typically reads `.env` automatically, but if on Windows PowerShell, ensure the variables are loaded).*

4. **Start API Server**
   - Start the backend API asynchronously.
   - PowerShell command: `Get-Content .env | ForEach-Object { if ($_ -match '^\s*([^#]\w*)\s*=\s*(.*)') { [Environment]::SetEnvironmentVariable($matches[1], $matches[2]) } }; cargo run -p api`
   - Use the `command_status` tool to ensure it compiles and begins listening on `0.0.0.0:8080`.

5. **Bootstrap Workspace & Key**
   - Execute a POST request to create a workspace:
     `curl.exe -s -X POST http://localhost:8080/v1/workspaces -H "Content-Type: application/json" -d '{\"name\": \"Local Dev Workspace\"}'`
   - Extract the `workspace_id` from the JSON response.
   - Execute a POST request to generate the API key for that workspace:
     `curl.exe -s -X POST http://localhost:8080/v1/workspaces/<workspace_id>/keys -H "Content-Type: application/json" -d '{\"name\": \"Dev Key\"}'`
   - Extract the `key` (plaintext key) from the JSON response.

6. **Configure Frontend**
   - Read `frontend/.env.local` and replace the `VITE_MEMORYOPS_WORKSPACE_ID=...` line with the newly generated `workspace_id`.
   - Use the `replace_file_content` tool to accomplish this efficiently.

7. **Start Frontend Dev Server**
   - Start the frontend asynchronously:
     `cd frontend; npm install; npm run dev`
   - Verify it initializes and binds to port `5173`.

8. **Handover to User**
   - Inform the user that the environment is successfully spun up.
   - Provide them with the **API Key** explicitly in your response.
   - Instruct them to navigate to `http://localhost:5173/` and paste the API key into the "Or use existing" input field on the setup screen to enter the app.
