# MemoryOps

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
│  │  Ingestion   │ → │  Processor   │ → │  Retrieval │  │
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

## Design Decisions

| Decision | Choice | Rationale |
|----------|--------|-----------|
| Embedding model | Pluggable (`EmbeddingProvider` trait) — local default via `fastembed-rs` | No external dependency required; swap to OpenAI via config |
| LLM for summarization | Pluggable (`LlmProvider` trait) — local default via Ollama | Self-hostable by default; OpenAI/Anthropic via config |
| Authentication | API key per workspace (v0.1) | Simple, secure, no OAuth complexity until SaaS phase |
| Retrieval mode | Pull — agent calls `/retrieve` | Simpler integration; push/middleware layer is a future SDK concern |
| Memory scope | Configurable: workspace / agent / user / repo | Operators define scope hierarchy in workspace config |
| Webhook validation | HMAC-SHA256 (GitHub-style) for all sources | Consistent, battle-tested; Slack adapter maps to same interface |

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
| Frontend | React + TypeScript |

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
│   └── FEATURES.md      # Full feature list with status
├── docker-compose.yml
├── Cargo.toml           # Workspace root
└── README.md
```

---

## Key Differentiators

1. **Pluggable AI providers** — run fully local (Ollama + fastembed), swap to cloud with one config change
2. **Flexible memory scope** — workspace, agent, user, or repo-level — operator-configured
3. **Memory lifecycle** — events promote to semantic memory, decay over time, get pruned
4. **Token-aware retrieval** — not "top 5 chunks" but optimized context under real token budgets
5. **Memory Control Center** — inspect, edit, audit, and debug what your agent remembers and why

---

## Status

🚧 Active development — pre-alpha

---

## License

MIT
