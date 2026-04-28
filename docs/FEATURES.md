# MemoryOps — Feature List

**Version:** 0.7.0  
**Last Updated:** 2026-04-27

This document tracks all planned features across the platform with current status and target milestone.

---

## Status Key

| Symbol | Meaning |
|--------|---------|
| 🔴 | Not started |
| 🟡 | In progress |
| 🟢 | Complete |

---

## Milestone Progress

| Milestone | Scope | Status |
|-----------|-------|--------|
| M1 | Scaffold & infrastructure | 🟢 Complete |
| M2 | GitHub ingestion | 🟢 Complete |
| M3 | Fast path processor | 🟢 Complete |
| M4 | Retrieval engine + providers | 🟢 Complete |
| M5 | React Control Center (frontend) | 🟢 Complete |
| M6 | Auth, rate limiting, workspace API, lifecycle, audit | 🟢 Complete |
| M7 | Slow path worker, embeddings, Qdrant writes, scheduler | 🟢 Complete |
| M8 | Promotion pipeline | 🟢 Complete |
| M9 | Slack ingestion | 🔴 Not started |

---

## Scaffold & Infrastructure

| Feature | Status | Milestone | Notes |
|---------|--------|-----------|-------|
| Cargo workspace root | 🟢 | M1 | 5 crates wired |
| docker-compose (dev) | 🟢 | M1 | Postgres, Redis, Qdrant |
| docker-compose.test.yml | 🟢 | M1 | Isolated test infra |
| sqlx migrations scaffold | 🟢 | M1 | 0001–0010 applied |
| rust-toolchain.toml (MSRV 1.88) | 🟢 | M1 | |
| GitHub Actions CI (fmt + clippy + test + integration) | 🟢 | M1/M6 | Integration job added in M6 |
| common crate (models, traits, error, config, telemetry) | 🟢 | M1 | |

---

## Ingestion

| Feature | Status | Milestone | Notes |
|---------|--------|-----------|-------|
| GitHub webhook receiver | 🟢 | M2 | HMAC-SHA256 validation |
| GitHub event normalization (PR, push, review, issue) | 🟢 | M2 | Maps to RawEvent |
| HMAC signature validation (shared WebhookValidator trait) | 🟢 | M2 | GitHub + Slack via same interface |
| RawEvent Postgres write (transactional) | 🟢 | M2 | |
| Redis job enqueue on ingest | 🟢 | M2 | |
| Idempotency key (SHA256 dedup) | 🟢 | M2 | |
| Integration health tracking (last_event_at, error_count) | 🟢 | M6 | integrations table |
| Dead letter queue for failed jobs | 🟢 | M6 | Redis list dlq:{workspace_id} |
| DLQ manual retry API | 🟢 | M6 | POST /workspaces/:id/dlq/:job_id/retry |
| Auto-retry with exponential backoff (max 3) | 🟢 | M6 | |
| Slack webhook receiver | 🔴 | M9 | message, thread, reaction |

---

## Processing

| Feature | Status | Milestone | Notes |
|---------|--------|-----------|-------|
| Fast path worker (no LLM) | 🟢 | M3 | Entity extraction, tagging, importance scoring |
| Entity extraction (regex + rules) | 🟢 | M3 | Person, Repo, Branch, File, Team |
| Importance scoring (rule table) | 🟢 | M3 | Workspace-configurable thresholds |
| Episodic MemoryUnit creation | 🟢 | M3 | |
| Slow path async worker | 🟢 | M7 | XREADGROUP consumer loop; LLM + embed pipeline |
| LLM summarization (OllamaProvider default) | 🟢 | M7 | Via LlmProvider trait |
| Embedding generation (fastembed-rs default) | 🟢 | M7 | Via EmbeddingProvider trait |
| Qdrant vector write | 🟢 | M7 | memoryops_memories collection |
| Decay scheduler (daily 02:00 UTC) | 🟢 | M7 | Spawned from api/main.rs |
| Pruning (soft delete at decay < 0.1) | 🟢 | M7 | Skips pinned + importance_overridden |
| Hard delete after 30-day window | 🟢 | M7 | Also removes Qdrant point |
| Promotion pipeline (episodic → semantic) | 🟢 | M8 | Cluster → threshold → summarize → promote |
| Configurable promotion threshold | 🟢 | M8 | Per workspace |
| Memory deduplication | 🟢 | M8 | Cosine similarity threshold |

---

## AI Providers (Pluggable)

| Feature | Status | Milestone | Notes |
|---------|--------|-----------|-------|
| EmbeddingProvider trait | 🟢 | M4 | In common crate |
| FastEmbedProvider (local, default) | 🟢 | M4 | fastembed-rs |
| OpenAIEmbedProvider | 🟢 | M4 | text-embedding-3-small |
| LlmProvider trait | 🟢 | M4 | In common crate |
| OllamaProvider (local, default) | 🟢 | M4 | Configurable model |
| OpenAIProvider | 🟢 | M4 | Chat Completions API |
| AnthropicProvider | 🟢 | M4 | Messages API |
| Provider selection via TOML config | 🟢 | M4 | No code changes to swap |

---

## Retrieval Engine

| Feature | Status | Milestone | Notes |
|---------|--------|-----------|-------|
| Hybrid search (RRF: Qdrant + Postgres FTS) | 🟢 | M4 | Degrades gracefully to keyword-only without embeddings |
| Keyword search (PostgreSQL tsvector + GIN) | 🟢 | M4 | plainto_tsquery, FTS indexes |
| RRF fusion scoring | 🟢 | M4 | k=60, normalized 0–1 |
| Decay scoring formula | 🟢 | M4 | importance × 0.5^(elapsed/half-life) |
| Decay batch update pass | 🟢 | M4 | apply_decay_scores_with_half_life; skips pinned/overridden |
| Access tracking (Redis HINCR + last_accessed_at) | 🟢 | M4 | 90-day TTL |
| Promotion eligibility check (access + importance) | 🟢 | M4 | Async, non-blocking |
| GET /v1/memory (list, filter, sort, paginate) | 🟢 | M4 | |
| GET /v1/memory/:id | 🟢 | M4 | |
| PATCH /v1/memory/:id (pin, tag, importance override) | 🟢 | M4 | |
| POST /v1/memory/search | 🟢 | M4 | |
| Token packing (greedy, under budget) | 🟢 | M6 | |
| Deduplication in packing (cosine > 0.92) | 🟢 | M6 | |
| POST /v1/retrieve (full retrieval surface) | 🟢 | M6 | With token packing + trace |
| Retrieval trace generation | 🟢 | M6 | Per-component scores, exclusion reasons |
| Trace persistence (30 days) | 🟢 | M6 | retrieval_traces table |
| GET /v1/retrieve/trace/:query_id | 🟢 | M6 | |
| Vector search leg (live when embedding_id populated) | 🟢 | M7 | Wired through slow path embeddings |
| Configurable scoring weights per workspace | 🟢 | M6 | WorkspaceConfig |

---

## API

| Feature | Status | Milestone | Notes |
|---------|--------|-----------|-------|
| GET /v1/memory | 🟢 | M4 | Paginated, filterable, sortable |
| GET /v1/memory/:id | 🟢 | M4 | |
| PATCH /v1/memory/:id | 🟢 | M4 | Pin, tag, edit, importance override |
| POST /v1/memory/search | 🟢 | M4 | Hybrid / vector / keyword |
| DELETE /v1/memory/:id | 🟢 | M6 | Soft delete; Qdrant point removed |
| POST /v1/memory/:id/promote | 🟢 | M6 | Force episodic → semantic |
| POST /v1/memory/:id/restore | 🟢 | M6 | Re-enqueues for re-embedding in M7 |
| POST /v1/memory/bulk | 🟢 | M6 | Bulk pin / unpin / delete (max 100) |
| GET /v1/memory/:id/history | 🟢 | M6 | Version history |
| POST /v1/memory/merge | 🟢 | M6 | Appends source → target, soft-deletes source |
| POST /v1/retrieve | 🟢 | M6 | |
| GET /v1/retrieve/trace/:query_id | 🟢 | M6 | |
| POST /v1/workspaces | 🟢 | M6 | Bootstrap, no auth required |
| GET /v1/workspaces/:id | 🟢 | M6 | |
| PATCH /v1/workspaces/:id/config | 🟢 | M6 | Scoring weights, thresholds, provider config |
| POST /v1/workspaces/:id/integrations | 🟢 | M6 | |
| GET /v1/workspaces/:id/integrations | 🟢 | M6 | With health status |
| DELETE /v1/workspaces/:id/integrations/:source | 🟢 | M6 | |
| POST /v1/workspaces/:id/keys | 🟢 | M6 | API key creation |
| GET /v1/workspaces/:id/keys | 🟢 | M6 | |
| DELETE /v1/workspaces/:id/keys/:key_id | 🟢 | M6 | Revoke |
| GET /v1/workspaces/:id/audit | 🟢 | M6 | Paginated audit log |
| GET /v1/workspaces/:id/dlq | 🟢 | M6 | |
| POST /v1/workspaces/:id/dlq/:job_id/retry | 🟢 | M6 | |
| DELETE /v1/workspaces/:id/dlq/:job_id | 🟢 | M6 | |
| GET /v1/workspaces/:id/export | 🟢 | M7 | JSONL streaming, cursor-paginated |
| POST /v1/workspaces/:id/promote | 🟢 | M8 | Manual promotion pass with workspace lock |

---

## Auth & Security

| Feature | Status | Milestone | Notes |
|---------|--------|-----------|-------|
| API key auth (X-API-Key header) | 🟢 | M6 | Middleware on all routes except /ingest/* + /health/* |
| Argon2id key hashing | 🟢 | M6 | Plaintext returned once on creation |
| Key format: mops_<prefix>_<32 bytes base58> | 🟢 | M6 | |
| Async last_used_at update (fire-and-forget) | 🟢 | M6 | |
| Key revocation | 🟢 | M6 | |
| Rate limiting (per workspace, per endpoint group) | 🟢 | M6 | Redis sliding window, 60s |
| 429 response with Retry-After header | 🟢 | M6 | |
| Rate limit groups: ingest 300 RPM, memory 120 RPM, api 120 RPM | 🟢 | M6 | |

---

## Audit & Observability

| Feature | Status | Milestone | Notes |
|---------|--------|-----------|-------|
| Audit log (all state-changing operations) | 🟢 | M6 | AuditEntry with before/after diff |
| Audit writes fire-and-forget (never block request path) | 🟢 | M6 | tokio::spawn |
| tracing + OpenTelemetry instrumentation | 🟢 | M6 | All services |
| Integration health dashboard data | 🟢 | M6 | |

---

## Frontend — Memory Control Center

| Feature | Status | Milestone | Notes |
|---------|--------|-----------|-------|
| Project scaffold (Vite + React 19 + TS + Tailwind v4) | 🟢 | M5 | |
| Zustand store (workspaceId + apiKey, in-memory only) | 🟢 | M5 | No localStorage/sessionStorage |
| TanStack Query API client wired to live endpoints | 🟢 | M5 | Workspace-scoped query keys |
| Vite proxy for /v1 and /health | 🟢 | M5 | |
| Memory Explorer (search, filter, sort, paginate) | 🟢 | M5 | GET /v1/memory + POST /v1/memory/search |
| Memory Detail view (entities, scope, score, tags) | 🟢 | M5 | GET /v1/memory/:id |
| Pin / Tag / Importance override actions | 🟢 | M5 | PATCH /v1/memory/:id (optimistic cache update) |
| Webhook tester (fire real GitHub payloads) | 🟢 | M5 | POST /v1/ingest/github |
| Settings view (workspace config, read-only) | 🟢 | M5 | |
| Retrieval Trace view (stubbed) | 🟢 | M5 | Wired in M8 |
| Lifecycle / Promotion Timeline (stubbed) | 🟢 | M5 | Wired in M8 |
| Integration Status view (stubbed) | 🟢 | M5 | |
| Audit Log view (stubbed) | 🟢 | M5 | |
| First-run workspace + key creation flow | 🟢 | M7 | POST /v1/workspaces + POST /v1/workspaces/:id/keys |
| X-API-Key header on all API requests | 🟢 | M7 | From Zustand store |
| Audit Log view (live data) | 🟢 | M7 | GET /v1/workspaces/:id/audit |
| Integration Status view (live data + DLQ panel) | 🟢 | M7 | |
| Export trigger (download JSONL) | 🟢 | M7 | GET /v1/workspaces/:id/export |
| Bulk pin / bulk delete | 🟢 | M6 | POST /v1/memory/bulk |
| Memory merge UI | 🟢 | M6 | POST /v1/memory/merge |
| Promotion controls | 🟢 | M8 | Threshold sliders + manual trigger |
| Semantic memory display | 🟢 | M8 | Badge, source count, promoted timestamp |

---

## Lifecycle Management

| Feature | Status | Milestone | Notes |
|---------|--------|-----------|-------|
| Decay scoring (daily scheduler 02:00 UTC) | 🟢 | M7 | Runs for all workspaces |
| Soft delete / archive at decay threshold (< 0.10) | 🟢 | M7 | Skips pinned + importance_overridden |
| 30-day recovery window | 🟢 | M6 | POST /v1/memory/:id/restore |
| Hard delete after recovery window | 🟢 | M7 | Also removes Qdrant point |
| Pin = decay freeze | 🟢 | M4 | Pinned skipped in decay pass |
| Manual importance override | 🟢 | M4 | Stored, used in scoring, skips decay |
| Memory version history | 🟢 | M6 | Incremented on edit and merge |
| Soft delete → Qdrant point removed immediately | 🟢 | M7 | embedder.delete_point() on DELETE |
| Restore → re-enqueue for re-embedding | 🟢 | M7 | processor_jobs stream |

---

## Export & Backup

| Feature | Status | Milestone | Notes |
|---------|--------|-----------|-------|
| JSONL export (streaming, cursor-paginated) | 🟢 | M7 | 500 rows/chunk, no raw payloads or vectors |
| Import (restore) | 🔴 | v1.0 | Not in v0.x scope |

---

## Database Migrations

| Migration | Description | Status |
|-----------|-------------|--------|
| 0001_init.sql | Core schema, workspaces base | 🟢 |
| 0002_memory_units.sql | memory_units + tsvector | 🟢 |
| 0003_raw_events.sql | raw_events + idempotency | 🟢 |
| 0004_retrieval.sql | retrieval_traces, access log | 🟢 |
| 0005_workspaces.sql | workspaces + workspace_config JSONB | 🟢 |
| 0006_api_keys.sql | api_keys table | 🟢 |
| 0007_audit_log.sql | audit_log table | 🟢 |
| 0008_integrations.sql | integrations + webhook_secret_hash | 🟢 |
| 0009_retrieval_traces.sql | retrieval_traces + 30-day TTL | 🟢 |
| 0010_soft_delete.sql | deleted_at + soft-delete indexes | 🟢 |
| 0011_scheduler.sql | hard_deleted_at + pruning indexes | 🟢 M7 |
| 0012_promotion.sql | semantic promotion metadata + thresholds | 🟢 M8 |
