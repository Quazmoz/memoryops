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
|-------|---------------|
| **Ingestion** | Connects to GitHub, Slack, Jira — pulls structured activity via webhooks |
| **Processing** | Normalizes events, extracts entities, scores importance (fast + async LLM paths) |
| **Memory Lifecycle** | Episodic → Semantic promotion, decay, deduplication, pruning |
| **Retrieval Engine** | Token-aware context optimization with hybrid semantic + BM25 search |
| **Control UI** | Memory explorer, pin/delete/merge, retrieval trace, audit log |

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
│  ┌──────────────────────────────────────────────────┐   │
│  │           Memory Control Center (React UI)       │   │
│  └──────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────┘
```

---

## Prerequisites

- [Rust](https://rustup.rs/) (stable, see `rust-toolchain.toml`)
- [Docker](https://www.docker.com/) + Docker Compose
- [Node.js](https://nodejs.org/) 20+ (frontend only)
- [sqlx-cli](https://github.com/launchbakery/sqlx-cli): `cargo install sqlx-cli`

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
```

The API will be available at `http://localhost:8080`.  
The frontend (if started) will be available at `http://localhost:5173`.

### Verify it's running

```bash
curl http://localhost:8080/health/ready
# {"status": "ok"}
```

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

## API Usage Example

### 1. Create a workspace

```bash
curl -X POST http://localhost:8080/v1/workspaces \
  -H 'Content-Type: application/json' \
  -d '{"name": "acme-engineering"}'
# {"id": "018f...", "name": "acme-engineering"}
```

### 2. Create an API key

```bash
curl -X POST http://localhost:8080/v1/workspaces/018f.../keys \
  -H 'X-API-Key: <admin-key>' \
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
    "token_budget": 4096,
    "filters": { "sources": ["github", "slack"] }
  }'
```

Response includes scored memories + a retrieval trace showing **why** each memory was selected.

---

## Design Decisions

| Decision | Choice | Rationale |
|----------|--------|-----------|
| Embedding model | Pluggable (`EmbeddingProvider` trait) — local default via `fastembed-rs` | No external dependency required; swap to OpenAI via config |
| LLM for summarization | Pluggable (`LlmProvider` trait) — local default via Ollama | Self-hostable by default; OpenAI/Anthropic via config |
| Authentication | API key per workspace (`X-API-Key` header) | Simple, no OAuth complexity in v0.1 |
| Retrieval mode | Pull — agent calls `POST /retrieve` | Simpler integration; push/middleware layer is a future SDK concern |
| Memory scope | Configurable: workspace / agent / user / repo | Operators define scope hierarchy in workspace config |
| Webhook validation | HMAC-SHA256 (GitHub-style) for all sources | Consistent, battle-tested |

---

## Tech Stack

| Component | Technology |
|-----------|------------|
| API Server | `axum` |
| Async Runtime | `tokio` |
| Database | PostgreSQL via `sqlx` |
| Vector Search | Qdrant |
| Full-Text Search | Tantivy (BM25) |
| Queue / Cache | Redis |
| Embeddings | `fastembed-rs` (local default) / pluggable |
| LLM Summarization | Ollama (local default) / pluggable |
| Observability | `tracing` + OpenTelemetry |
| Frontend | React 19 + TypeScript + Vite |

---

## Workspace Structure

```
memoryops/
├── crates/
│   ├── ingestion/       # Webhook receivers (GitHub, Slack)
│   ├── processor/       # Fast path + async slow path workers
│   ├── retrieval/       # Scoring, hybrid search, token packing
│   ├── api/             # Public REST API (axum)
│   └── common/          # Shared types, schemas, DB models, provider traits
├── frontend/            # React Memory Control Center
├── migrations/          # sqlx DB migrations
├── docs/
│   ├── SPEC.md          # Full technical specification
│   └── openapi.yaml     # API contract (source of truth)
├── .env.example
├── docker-compose.yml
├── docker-compose.test.yml
├── rust-toolchain.toml
├── Cargo.toml           # Workspace root
└── README.md
```

---

## Key Differentiators

1. **Multi-tool ingestion** — GitHub, Slack, Jira normalized into a single memory model
2. **Memory lifecycle** — events promote to semantic memory, decay over time, get pruned automatically
3. **Token-aware retrieval** — not "top 5 chunks" but greedy context packing under real token budgets with deduplication
4. **Pluggable AI providers** — run fully local (Ollama + fastembed), swap to cloud with one config change
5. **Memory Control Center** — inspect, edit, pin, merge, and trace exactly what your agent remembers and why

---

## Status

🚧 **Pre-alpha** — active development, not yet production-ready.

See [docs/SPEC.md](docs/SPEC.md) for the full technical specification and milestone roadmap.

---

## Contributing

1. Fork the repo and create a `feature/your-feature` branch
2. Follow [Conventional Commits](https://www.conventionalcommits.org/) for commit messages
3. Run `cargo fmt`, `cargo clippy -- -D warnings`, and `cargo test` before pushing
4. Open a PR — describe what changed, why, and how to test it
5. PRs require CI to pass and reference an issue or milestone

See [docs/SPEC.md §25](docs/SPEC.md#25-code-quality-standards) for full code quality standards.

---

## License

MIT
