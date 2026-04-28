# Local Development Guide

This guide covers how to spin up and test MemoryOps locally from scratch.

## Prerequisites

- **Rust 1.88.0** (`rustup` will auto-install this via the `rust-toolchain.toml` file)
- **Docker** + **Docker Compose**
- **sqlx-cli** (install via: `cargo install sqlx-cli --no-default-features --features rustls,postgres`)
- **Node.js 20+** and **npm**
- **Ollama** (for local LLM summarization — the default provider in `config.toml` is `ollama/llama3` at `http://localhost:11434`)
- *Optional:* OpenAI or Anthropic API key if you prefer switching providers.

---

## 1. Clone and configure environment

Start by copying the example environment variable files:

```bash
cp .env.example .env
```

**Required vs Optional Variables:**
- Core connection strings (`DATABASE_URL`, `REDIS_URL`, `QDRANT_URL`) and server details (`APP_HOST`, `APP_PORT`) are **required**.
- `APP_ENV=development` enables GitHub webhook secret fallback to ease local testing.
- Webhook secrets (`GITHUB_WEBHOOK_SECRET`, `SLACK_SIGNING_SECRET`, etc.) default to `dev-placeholder` for local use out of the box.
- AI Provider keys (`OPENAI_API_KEY`, `ANTHROPIC_API_KEY`) are optional unless you configure those providers in `config.toml`.

Next, configure the frontend:

```bash
cp frontend/.env.example frontend/.env.local
```

The `VITE_MEMORYOPS_WORKSPACE_ID` is set to a placeholder. You will need to update this to match a real workspace UUID after bootstrapping (Step 5).

---

## 2. Start infrastructure services

MemoryOps relies on PostgreSQL, Redis, and Qdrant. Start them in the background using Docker Compose:

```bash
docker compose up -d
```

- **PostgreSQL (5432):** Main relational database. Health check ensures `pg_isready`.
- **Redis (6379):** Backs the processor queues and caching.
- **Qdrant (6333 / 6334):** Vector search backend. Exposes HTTP on 6333 and gRPC on 6334.

**Note on Data Persistence:**
These services use named Docker volumes (`postgres-data`, `redis-data`, `qdrant-data`) which persist your data across restarts.

**Optional MCP Server:**
If you want to run the optional MCP profile locally, use:
```bash
docker compose --profile mcp up -d
```

---

## 3. Run database migrations

With your infrastructure running and your `DATABASE_URL` correctly set in `.env`, run the database migrations:

```bash
source .env # Ensure DATABASE_URL is available
sqlx migrate run
```

This applies all 14 migrations in order, covering: `init schema`, `ingestion indexes`, `processor`, `retrieval`, `workspaces`, `api_keys`, `audit_log`, `integrations`, `retrieval_traces`, `soft_delete`, `scheduler`, `promotion`, `slack`, and `linear_jira`.

---

## 4. Start the API server

Run the API server from the root of the workspace:

```bash
cargo run -p api
```
*(Optionally, explicitly point to the config: `cargo run -p api -- --config config.toml`)*

The server will bind to `APP_HOST:APP_PORT` (default `0.0.0.0:8080`).

**Note:** The first run will compile all 6 workspace crates (`api`, `common`, `ingestion`, `mcp`, `processor`, `retrieval`), which typically takes a 2-5 minute cold build.

Verify the server is up:
```bash
curl http://localhost:8080/health
```

---

## 5. Bootstrap a workspace

Before you can use the frontend, you must bootstrap a workspace (this endpoint requires no authentication):

```bash
curl -X POST http://localhost:8080/v1/workspaces \
  -H "Content-Type: application/json" \
  -d '{"name": "Local Dev Workspace"}'
```

Capture the returned `id` (e.g., `123e4567-e89b-12d3-a456-426614174000`), and use it to create an API key:

```bash
curl -X POST http://localhost:8080/v1/workspaces/123e4567-e89b-12d3-a456-426614174000/keys \
  -H "Content-Type: application/json" \
  -d '{"name": "Dev Key"}'
```

1. Update `frontend/.env.local` by setting `VITE_MEMORYOPS_WORKSPACE_ID` to your real workspace UUID.
2. Save the API key generated from the second command; you will need it for the frontend.

---

## 6. Start the frontend

Open a new terminal, install dependencies, and start the Vite dev server:

```bash
cd frontend
npm install
npm run dev
```

- The app is available at `http://localhost:5173`.
- The Vite config proxies `/v1` directly to `http://localhost:8080`, preventing CORS issues.
- **On first load:** Paste the API key into the Settings modal. The frontend does not persist this to `localStorage` — it is kept only in memory via Zustand.

---

## 7. Running tests

To run unit and integration tests:

```bash
cargo test
```

**Integration Tests:**
These tests require the ephemeral test stack. Start it up:

```bash
docker compose -f docker-compose.test.yml up -d
```

- The test stack uses separate ports (`15432`, `16379`, `16333`/`16334`) to prevent collision with your main dev stack.
- It uses `tmpfs` volumes, meaning the data is ephemeral and automatically wiped when containers stop.

Run tests using the test database connection:
```bash
DATABASE_URL=postgres://memoryops:memoryops@localhost:15432/memoryops_test cargo test
```

**Frontend Tests:**
```bash
cd frontend
npm run test
```

---

## 8. Linting and formatting

MemoryOps enforces strict workspace-level lints (denying `unwrap_used`, `expect_used`, `dbg_macro`, and `todo`).

Format code:
```bash
cargo fmt --all
```

Run Clippy:
```bash
cargo clippy -p api -p common -p ingestion -p processor -p retrieval -p mcp -- -D warnings
```

---

## 9. Local LLM setup (Ollama)

The slow path uses an LLM for summarization. The default provider configured in `config.toml` is `ollama` using the `llama3` model.

Install Ollama, then run:
```bash
ollama pull llama3
ollama serve
```

- **If Ollama is not running:** Slow-path summarization jobs will fail and enter the Dead Letter Queue (DLQ).
- **Retrying jobs:** You can retry jobs via `POST /v1/dlq/:id/retry` or using the DLQ UI at `/dlq`.
- **To skip LLM in Dev:** Edit `config.toml`, change `[llm]` provider to `"openai"`, set `OPENAI_API_KEY` in `.env`, or simply let DLQ items accumulate.

---

## 10. Using the Ingest Tester

To test incoming webhook payloads:
1. Navigate to `http://localhost:5173/ingest` in the UI.
2. Send test webhook payloads for GitHub, Slack, Linear, or Jira.
3. Webhook signature validation relies on the `dev-placeholder` secrets, which works out of the box when `APP_ENV=development`.

---

## 11. Port reference table

| Service | Port | Notes |
|---|---|---|
| API server | `8080` | axum, REST |
| Frontend | `5173` | Vite dev server |
| PostgreSQL | `5432` | main dev DB |
| Redis | `6379` | queue + cache |
| Qdrant HTTP | `6333` | vector REST API |
| Qdrant gRPC | `6334` | used by Rust client |
| MCP server | `3003` | optional, `--profile mcp` |
| Test PostgreSQL | `15432` | `docker-compose.test.yml` |
| Test Redis | `16379` | `docker-compose.test.yml` |
| Test Qdrant | `16333`/`16334` | `docker-compose.test.yml` |

---

## 12. Common issues and fixes

- **Port conflict on 5432:** Run `lsof -i :5432` to see what is using the port. Stop your local Postgres instance, or remap the port in `docker-compose.yml`.
- **Migrations fail:** Ensure `DATABASE_URL` is exported in your active shell and the Postgres container is healthy (`docker compose ps`).
- **Slow path jobs stuck in DLQ:** Make sure Ollama is running and the model is pulled (`ollama pull llama3`).
- **Frontend 404 on `/v1`:** The Vite proxy requires the API server to be running on `8080`. Check that `cargo run -p api` is successfully running.
- **Qdrant connection refused:** Ensure the gRPC port `6334` is bound and exposed in `docker-compose.yml` (the Rust client connects via gRPC).
