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
| **Ingestion** | Connects to GitHub, Slack, Jira — pulls structured activity |
| **Processing** | Normalizes events, extracts entities, scores importance |
| **Memory Lifecycle** | Episodic → Semantic promotion, decay, deduplication, pruning |
| **Retrieval Engine** | Token-aware context optimization with hybrid semantic + BM25 search |
| **Control UI** | Memory explorer, pin/delete/merge, retrieval trace (why did the agent answer that?) |

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
│       Postgres           Redis Queue         Qdrant     │
│       (events)           (async jobs)     (embeddings)  │
│                                                         │
│  ┌──────────────────────────────────────────────────┐   │
│  │           Memory Control Center (React UI)       │   │
│  └──────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────┘
```

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
│   └── common/          # Shared types, schemas, DB models
├── frontend/            # React Memory Control Center
├── migrations/          # sqlx DB migrations
├── docs/
│   └── SPEC.md          # Full technical specification
├── docker-compose.yml
├── Cargo.toml           # Workspace root
└── README.md
```

---

## Key Differentiators

1. **Multi-tool ingestion** — unified memory from GitHub, Slack, Jira (not just one source)
2. **Memory lifecycle** — events promote to semantic memory, decay over time, get pruned
3. **Token-aware retrieval** — not "top 5 chunks" but optimized context under real token budgets
4. **Memory Control Center** — inspect, edit, and debug what your agent remembers and why

---

## Status

🚧 Active development — pre-alpha

---

## License

MIT
