# Skill: MemoryOps Local Docker Environment

You are a local dev assistant for the **MemoryOps** project — a Rust/Axum API + React frontend
with Postgres, Redis, and Qdrant as backing services.

## Stack
- **Services**: postgres (16), redis (7), qdrant (latest), api (Rust/Axum), mcp (Rust), frontend (React/Vite)
- **Compose files**: `docker-compose.yml` (full stack), `docker-compose.test.yml` (integration test deps only)
- **Config**: `config.toml` — TOML config for AI providers, embedding, promotion settings
- **Env**: `.env` (copy from `.env.example`) — never committed

## Required env vars before `docker compose up`
| Var | Notes |
|-----|-------|
| `APP_SECRET_KEY` | **Required, no default.** Min 32-char random string. Used for AES-256-GCM secret encryption. Generate with: `openssl rand -base64 32` |
| `GITHUB_WEBHOOK_SECRET` | Defaults to `dev-placeholder` in dev — safe to omit locally |
| `VITE_MEMORYOPS_WORKSPACE_ID` | Optional at startup. Set after creating a workspace via `POST /v1/workspaces` then rebuild frontend |

## Startup sequence
```bash
# 1. Copy env file if missing
cp .env.example .env

# 2. Set the one required secret (if not already in .env)
echo "APP_SECRET_KEY=$(openssl rand -base64 32)" >> .env

# 3. Start infra first, wait for health
docker compose up -d postgres redis qdrant
docker compose wait postgres redis qdrant   # blocks until all healthy

# 4. Build and start the API (runs sqlx migrations on boot)
docker compose up -d api

# 5. Start MCP server
docker compose up -d mcp

# 6. Build and start frontend (slow — React build inside Docker)
docker compose up -d frontend

# 7. Tail logs
docker compose logs -f api mcp
```

## Port map
| Service | Port | URL |
|---------|------|-----|
| API | 8080 | http://localhost:8080 |
| Frontend | 5173 | http://localhost:5173 |
| MCP | 3003 | http://localhost:3003 |
| Postgres | 5432 | postgres://memoryops:memoryops@localhost:5432/memoryops |
| Redis | 6379 | redis://localhost:6379 |
| Qdrant HTTP | 6333 | http://localhost:6333 |
| Qdrant gRPC | 6334 | http://localhost:6334 |

## Common tasks

### Create a workspace (first-time setup)
```bash
curl -s -X POST http://localhost:8080/v1/workspaces \
  -H "Content-Type: application/json" \
  -H "x-admin-token: $WORKSPACE_CREATION_SECRET" \
  -d '{"name": "local-dev"}' | jq
# Copy the returned workspace_id into .env as VITE_MEMORYOPS_WORKSPACE_ID
# Then rebuild the frontend: docker compose up -d --build frontend
```

### Wipe and restart clean
```bash
docker compose down -v   # destroys all volumes (postgres, redis, qdrant data)
docker compose up -d
```

### Rebuild only the Rust API after code changes
```bash
docker compose up -d --build api mcp
```

### Run integration tests (separate compose file)
```bash
docker compose -f docker-compose.test.yml up -d
cargo test --workspace
docker compose -f docker-compose.test.yml down -v
```

### Check health of all services
```bash
docker compose ps
```

## Troubleshooting

| Symptom | Fix |
|---------|-----|
| `api` exits immediately | Check `APP_SECRET_KEY` is set in `.env`. The binary panics at startup if missing. |
| `api` can't connect to postgres | Postgres healthcheck must pass before api starts. Run `docker compose up -d postgres && docker compose wait postgres` then retry. |
| Frontend shows blank / 502 | API isn't healthy yet. Check `docker compose logs api`. |
| Qdrant collection missing | The API auto-creates the collection on first run. If it errors, check `QDRANT_URL` resolves to port `6334` (gRPC), not `6333` (HTTP). |
| Port already in use | Another service owns 5432/6379/6333/8080. Stop conflicting services or change ports in `docker-compose.yml`. |
| `WORKSPACE_CREATION_SECRET` not set | `create_workspace` is gated by `x-admin-token`. Set `WORKSPACE_CREATION_SECRET` in `.env` to any value locally. |

## Notes
- The API binary serves both migrations and the HTTP server — no separate migration step needed.
- `APP_ENV=development` relaxes some webhook secret validation. Don't use production keys locally.
- The `mcp` service uses HTTP transport on port 3003 by default (`MCP_TRANSPORT=http`).
- AI provider keys (`OPENAI_API_KEY`, `ANTHROPIC_API_KEY`) are only needed if `config.toml` points to those providers. Local dev defaults to whatever is configured in `config.toml`.