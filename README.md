# MemoryOps

[![CI](https://github.com/Quazmoz/memoryops/actions/workflows/ci.yml/badge.svg)](https://github.com/Quazmoz/memoryops/actions/workflows/ci.yml)
[![Coverage](https://img.shields.io/badge/coverage-80%25-brightgreen)](https://github.com/Quazmoz/memoryops)
[![Rust](https://img.shields.io/badge/rust-stable-orange)](https://www.rust-lang.org)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)
[![Status: Pre-Alpha](https://img.shields.io/badge/status-pre--alpha-red)](#status)

> The Memory Operations Platform for AI Agents

**MemoryOps** is a Rust-powered backend system that turns engineering activity (GitHub, Slack, Jira) into structured, controllable, token-optimized memory for AI agents.

This is **not** a vector database, RAG wrapper, or agent framework.  
This is the **control plane for what AI agents remember**.

---

## The Problem

AI agents today:
- Forget everything across sessions
- Rely on prompt-stuffing hacks
- Use naive top-K retrieval with no context optimization
- Have zero memory governance or lifecycle management

Result: inconsistent behavior, repeated instructions, hallucinations from stale context.

---

## What MemoryOps Does

| Layer | What It Solves |
|-------|----------------|
| **Ingestion** | Connects to GitHub, Slack, Jira, Linear — pulls structured activity via webhooks |
| **Processing** | Normalizes events, extracts entities, scores importance (fast + async LLM paths) |
| **Memory Lifecycle** | Episodic → Semantic promotion, decay, deduplication, pruning |
| **Retrieval Engine** | Token-aware context optimization with hybrid semantic + BM25 search |
| **Feedback Loop** | Explicit per-memory ratings bias future retrieval via rolling relevance scores |
| **Point-in-Time Queries** | Reconstruct exact memory state at any past timestamp for incident post-mortems |
| **Multi-Agent Memory** | Publish semantic memories to workspace pool; sub-agents inherit via config |
| **MCP Server** | Native Model Context Protocol server — agents retrieve, search, and store without HTTP |
| **Control UI** | Memory explorer, pin/delete/merge, retrieval trace, audit log, skills registry |

---

## Architecture

```
┌─────────────────────────────────────────────────────────┐
│                     MemoryOps Platform                  │
│                                                         │
│  ┌──────────────┐   ┌──────────────┐   ┌────────────┐  │
│  │  Ingestion   │──▶│  Processor   │──▶│  Retrieval │  │
│  │  (Webhooks)  │   │  Fast + Slow │   │   Engine   │  │
│  └──────────────┘   └──────────────┘   └────────────┘  │
│          │                  │                  │        │
│       Postgres           Redis Queue      Qdrant +      │
│       (events +          (async jobs)    Tantivy        │
│        memories)                        (hybrid search) │
│                                                         │
│  ┌──────────────┐   ┌──────────────────────────────┐   │
│  │  MCP Server  │   │  Memory Control Center (UI)  │   │
│  │  (port 3003) │   │  React 19 + TypeScript       │   │
│  └──────────────┘   └──────────────────────────────┘   │
└─────────────────────────────────────────────────────────┘
```

---

## Prerequisites

- [Rust](https://rustup.rs/) (stable, see `rust-toolchain.toml` — currently 1.88.0)
- [Docker](https://www.docker.com/) + Docker Compose
- [Node.js](https://nodejs.org/) 20+ (frontend only)
- [sqlx-cli](https://github.com/launchbakery/sqlx-cli): `cargo install sqlx-cli --no-default-features --features rustls,postgres`
- [Ollama](https://ollama.com/) (local LLM default — `ollama pull llama3`)

---

## Quick Start

```bash
# 1. Clone the repo
git clone https://github.com/Quazmoz/memoryops.git
cd memoryops

# 2. Start infrastructure (Postgres, Redis, Qdrant)
docker compose up -d

# 3. Copy and configure environment
cp .env.example .env
# Edit .env — set DATABASE_URL, REDIS_URL, QDRANT_URL at minimum

# 4. Run database migrations
sqlx migrate run

# 5. Build and run the API
cargo run -p api

# 6. (Optional) Start the frontend
cd frontend && npm install && npm run dev

# 7. (Optional) Seed development data
API_KEY=your-key bash scripts/seed.sh
```

The API will be available at `http://localhost:8080`.  
The frontend (if started) will be available at `http://localhost:5173`.  
The MCP server (optional profile) runs at `http://localhost:3003`.

### Verify it's running

```bash
curl http://localhost:8080/health/ready
# {"status": "ok"}
```

See [docs/local-development.md](docs/local-development.md) for a full step-by-step local setup guide including Ollama, test stack, and port reference.

---

## Connecting AI Clients

MemoryOps exposes MCP tools for AI clients via HTTP Streamable or stdio transport.

- **Open WebUI:** [docs/integrations/openwebui.md](docs/integrations/openwebui.md)
- **Claude Code:** [docs/integrations/claude-code.md](docs/integrations/claude-code.md)
- **GitHub Copilot in VS Code:** [docs/integrations/vscode.md](docs/integrations/vscode.md)
- **Continue.dev:** [docs/integrations/vscode.md](docs/integrations/vscode.md)

See [docs/mcp-transport.md](docs/mcp-transport.md) for the full transport reference and 
HTTP Streamable session lifecycle.

---

## Environment Variables

Copy `.env.example` to `.env`. All required variables must be set before starting.

| Variable | Required | Default | Description |
|----------|----------|---------|-------------|
| `DATABASE_URL` | ✅ | — | Postgres connection string (`postgres://user:pass@host/db`) |
| `REDIS_URL` | ✅ | — | Redis connection string (`redis://localhost:6379`) |
| `QDRANT_URL` | ✅ | — | Qdrant gRPC URL (`http://localhost:6334`) |
| `APP_HOST` | ❌ | `0.0.0.0` | API bind address |
| `APP_PORT` | ❌ | `8080` | API listen port |
| `APP_ENV` | ❌ | `development` | `development` or `production` |
| `CONFIG_PATH` | ❌ | `config.toml` | Path to TOML config file |
| `OPENAI_API_KEY` | ❌ | — | Required only if `embedding.provider = "openai"` |
| `ANTHROPIC_API_KEY` | ❌ | — | Required only if `llm.provider = "anthropic"` |
| `RUST_LOG` | ❌ | `info` | Log level (`trace`, `debug`, `info`, `warn`, `error`) |

Secrets are **never** stored in `config.toml` — always via environment variables.

---

## API Usage Examples

### 1. Create a workspace

```bash
curl -X POST http://localhost:8080/v1/workspaces \
  -H 'Content-Type: application/json' \
  -d '{"name": "acme-engineering"}'
# {"workspace_id": "018f...", "api_key": "mops_018f..._..."}
```

The bootstrap API key is returned once. Store it securely.

### 2. Create an additional API key

```bash
curl -X POST http://localhost:8080/v1/workspaces/018f.../keys \
  -H 'X-API-Key: mops_018f...' \
  -H 'Content-Type: application/json' \
  -d '{"name": "coding-agent"}'
# {"key": "mops_acme_3xK9m..."} ← returned once, store it
```

### 3. Register a GitHub webhook

In your GitHub repo settings, add a webhook pointing to:
```
https://your-host/v1/webhooks/github
```
With your signing secret matching `GITHUB_WEBHOOK_SECRET`.

### 4. Retrieve memory for an agent

```bash
curl -X POST http://localhost:8080/v1/retrieve \
  -H 'X-API-Key: mops_acme_3xK9m...' \
  -H 'Content-Type: application/json' \
  -d '{
    "query": "What are the recent decisions about the auth service?",
    "workspace_id": "018f...",
    "token_budget": 4096,
    "agent_id": "coding-agent"
  }'
```

Response includes scored, token-packed memories + a retrieval trace showing **why** each memory was selected.

### 5. Submit feedback on a retrieved memory

```bash
curl -X POST http://localhost:8080/v1/memory/019a.../feedback \
  -H 'X-API-Key: mops_acme_3xK9m...' \
  -H 'Content-Type: application/json' \
  -d '{
    "query_id": "trace-uuid-from-retrieve",
    "rating": 1,
    "agent_id": "coding-agent",
    "comment": "Exactly the context I needed"
  }'
```

Ratings (`-1`, `0`, `1`) roll into a `relevance_score` that nudges future hybrid retrieval rankings.

### 6. Query memory at a past timestamp

```bash
curl -X POST http://localhost:8080/v1/retrieve \
  -H 'X-API-Key: mops_acme_3xK9m...' \
  -H 'Content-Type: application/json' \
  -d '{
    "query": "auth service decisions",
    "workspace_id": "018f...",
    "as_of": "2026-04-15T00:00:00Z"
  }'
```

---

## Design Decisions

| Decision | Choice | Rationale |
|----------|--------|-----------| 
| Embedding model | Pluggable (`EmbeddingProvider` trait) — local default via `fastembed-rs` | No external dependency required; swap to OpenAI via config |
| LLM for summarization | Pluggable (`LlmProvider` trait) — local default via Ollama | Self-hostable by default; OpenAI/Anthropic via config |
| Authentication | API key per workspace (`X-API-Key` header) | Simple, no OAuth complexity in v0.x |
| Retrieval mode | Pull — agent calls `POST /retrieve` | Simpler integration; push/middleware layer is a future SDK concern |
| Memory scope | Configurable: workspace / agent / user / repo | Operators define scope hierarchy in workspace config |
| Multi-agent sharing | Opt-in publish (`POST /memory/:id/publish`) + workspace pool inheritance | Explicit promotion prevents accidental cross-agent leakage |
| Webhook validation | HMAC-SHA256 (GitHub-style) for all sources | Consistent, battle-tested |
| MCP transport | stdio + HTTP Streamable | Works with Claude Desktop, VS Code, Open WebUI, Continue.dev, and custom agent frameworks |

---

## Tech Stack

| Component | Technology |
|-----------|------------|
| API Server | `axum` |
| Async Runtime | `tokio` |
| Database | PostgreSQL via `sqlx` |
| Vector Search | Qdrant |
| Full-Text Search | Tantivy (BM25) |
| Queue / Cache | Redis Streams |
| Embeddings | `fastembed-rs` (local default) / pluggable |
| LLM Summarization | Ollama (local default) / pluggable |
| MCP Server | `rmcp` crate — stdio + HTTP Streamable (MCP spec 2025-03-26) |
| Observability | `tracing` + OpenTelemetry |
| Frontend | React 19 + TypeScript + Vite + Tailwind v4 |

---

## Workspace Structure

```
memoryops/
├── crates/
│   ├── api/             # Public REST API (axum handlers, middleware, routing)
│   ├── common/          # Shared types, DB models, provider traits, AppError
│   ├── ingestion/       # Webhook receivers (GitHub, Slack, Jira, Linear)
│   ├── mcp/             # MCP server (memory_retrieve, memory_search, memory_store tools)
│   ├── processor/       # Fast path + async slow path workers
│   └── retrieval/       # Hybrid search, RRF scoring, token packing, feedback scoring
├── frontend/            # React 19 Memory Control Center
├── migrations/          # sqlx DB migrations
├── scripts/
│   └── seed.sh          # Idempotent dev data seeding script
├── docs/
│   ├── integrations/    # MCP client setup guides for Open WebUI, VS Code, Continue.dev
│   ├── FEATURES.md      # Milestone tracker and full feature list
│   ├── SPEC.md          # Full technical specification
│   ├── openapi.yaml     # API contract (source of truth)
│   └── local-development.md  # Step-by-step local setup guide
├── .env.example
├── docker-compose.yml
├── docker-compose.test.yml
├── rust-toolchain.toml
├── Cargo.toml           # Workspace root
└── README.md
```

---

## Key Differentiators

1. **Multi-source ingestion** — GitHub, Slack, Jira, Linear normalized into a single memory model with HMAC-validated webhooks
2. **Memory lifecycle** — events promote to semantic memory, decay over time, get pruned automatically; all thresholds are per-workspace config
3. **Token-aware retrieval** — not "top 5 chunks" but greedy context packing under real token budgets with cosine deduplication
4. **Pluggable AI providers** — run fully local (Ollama + fastembed), swap to cloud with one config change, no code edits
5. **Retrieval feedback loop** — explicit thumbs-up/down ratings roll into a `relevance_score` that nudges hybrid scoring, closing the agent learning loop
6. **Agent-managed memory** — agents can delete, update, and rate memories directly via MCP tools (`memory_delete`, `memory_update`, `memory_feedback`), not just read and store
7. **Point-in-time memory** — reconstruct exact memory state + decay scores at any past timestamp; useful for incident post-mortems
8. **Temporal memory queries** — `memory_timeline` MCP tool lets agents reconstruct what they knew at any past timestamp — useful for incident post-mortems and audit
9. **Multi-agent scope inheritance** — publish semantic memories to a workspace pool; sub-agents inherit without duplicating context
10. **MCP-native** — agents connect directly via Model Context Protocol (stdio or HTTP Streamable); no HTTP client glue required
11. **Memory Control Center** — inspect, edit, pin, merge, trace, and audit exactly what your agent remembers and why

---

```bash
# 1. Clone the repo
git clone https://github.com/Quazmoz/memoryops.git
cd memoryops

# 2. Start infrastructure (Postgres, Redis, Qdrant)
docker compose up -d

# 3. Copy and configure environment
cp .env.example .env
# Edit .env — set DATABASE_URL, REDIS_URL, QDRANT_URL at minimum

# 4. Run database migrations
sqlx migrate run

# 5. Build and run the API
cargo run -p api

# 6. (Optional) Start the frontend
cd frontend && npm install && npm run dev

# 7. (Optional) Start the MCP server for AI client connections
#    IMPORTANT: override the docker-compose.yml default (which uses deprecated sse)
docker compose --profile mcp run --rm --service-ports \
  -e MCP_TRANSPORT=http \
  -e MCP_PORT=3003 \
  mcp

# 8. (Optional) Seed development data
API_KEY=your-key bash scripts/seed.sh
```

MIT
