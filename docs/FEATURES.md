# MemoryOps — Feature List

**Version:** 0.3.0  
**Last Updated:** 2026-04-27

This document tracks all planned features across the platform with current status and target milestone.

---

## Status Key

| Symbol | Meaning |
|--------|---------|
| 🔴 | Not started |
| 🟡 | In design |
| 🟢 | Complete |

---

## Scaffold & Infrastructure

| Feature | Status | Milestone | Notes |
|---------|--------|-----------|-------|
| Cargo workspace root | 🟢 | M1 | 5 crates wired |
| docker-compose (dev) | 🟢 | M1 | Postgres, Redis, Qdrant |
| docker-compose.test.yml | 🟢 | M1 | Isolated test infra |
| sqlx migrations scaffold | 🟢 | M1 | Numbered migrations |
| rust-toolchain.toml (MSRV 1.88) | 🟢 | M1 | |
| GitHub Actions CI (fmt + clippy + test) | 🟢 | M1 | |
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
| Slack webhook receiver | 🔴 | M9 | message, thread, reaction |
| Integration health tracking (last_event_at, error_count) | 🔴 | M6 | |
| Dead letter queue for failed jobs | 🔴 | M6 | Redis list |
| DLQ manual retry API | 🔴 | M6 | POST /workspaces/:id/dlq/:job_id/retry |
| Auto-retry with exponential backoff (max 3) | 🔴 | M6 | |
| Ingestion rate metrics | 🔴 | M6 | Per workspace, per source |

---

## Processing

| Feature | Status | Milestone | Notes |
|---------|--------|-----------|-------|
| Fast path worker (no LLM) | 🟢 | M3 | Entity extraction, tagging, importance scoring |
| Entity extraction (regex + rules) | 🟢 | M3 | Person, Repo, Branch, File, Team |
| Importance scoring (rule table) | 🟢 | M3 | Workspace-configurable thresholds |
| Episodic MemoryUnit creation | 🟢 | M3 | |
| Slow path async worker | 🔴 | M7 | Consumes Redis queue |
| LLM summarization (OllamaProvider default) | 🔴 | M7 | Via LlmProvider trait |
| Embedding generation (fastembed-rs default) | 🔴 | M7 | Via EmbeddingProvider trait |
| Qdrant vector write | 🔴 | M7 | |
| Promotion pipeline (episodic → semantic) | 🔴 | M8 | Cluster → threshold → summarize → promote |
| Configurable promotion threshold | 🔴 | M8 | Per workspace |
| Configurable clustering time window | 🔴 | M8 | Per workspace |
| Memory deduplication | 🔴 | M8 | Cosine similarity threshold |
| Decay scheduler (daily job) | 🔴 | M6 | Configurable decay rates |
| Pruning (soft delete at decay < 0.1) | 🔴 | M6 | |

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
| Decay batch update pass | 🟢 | M4 | applydecayscoreswithhalflife; skips pinned/overridden |
| Access tracking (Redis HINCR + last_accessed_at) | 🟢 | M4 | 90-day TTL |
| Promotion eligibility check (access + importance) | 🟢 | M4 | Async, non-blocking |
| GET /v1/memory (list, filter, sort, paginate) | 🟢 | M4 | |
| GET /v1/memory/:id | 🟢 | M4 | |
| PATCH /v1/memory/:id (pin, tag, importance override) | 🟢 | M4 | |
| POST /v1/memory/search | 🟢 | M4 | |
| Token packing (greedy, under budget) | 🔴 | M6 | |
| Deduplication in packing (cosine > 0.92) | 🔴 | M6 | |
| POST /v1/retrieve (full retrieval surface) | 🔴 | M6 | With token packing + trace |
| Retrieval trace generation | 🔴 | M6 | Per-component scores, exclusion reasons |
| Trace persistence (30 days) | 🔴 | M6 | Queryable by query_id |
| GET /v1/retrieve/trace/:query_id | 🔴 | M6 | |
| Configurable scoring weights per workspace | 🔴 | M6 | |
| Configurable source authority per source | 🔴 | M6 | |

---

## API

| Feature | Status | Milestone | Notes |
|---------|--------|-----------|-------|
| GET /v1/memory | 🟢 | M4 | Paginated, filterable, sortable |
| GET /v1/memory/:id | 🟢 | M4 | |
| PATCH /v1/memory/:id | 🟢 | M4 | Pin, tag, edit, importance override |
| POST /v1/memory/search | 🟢 | M4 | Hybrid / vector / keyword |
| DELETE /v1/memory/:id | 🔴 | M6 | Soft delete |
| POST /v1/memory/:id/promote | 🔴 | M6 | |
| POST /v1/memory/:id/restore | 🔴 | M6 | |
| POST /v1/memory/bulk | 🔴 | M6 | Bulk pin / bulk delete |
| GET /v1/memory/:id/history | 🔴 | M6 | Version history |
| POST /v1/memory/merge | 🔴 | M6 | Merge two semantic memories |
| POST /v1/retrieve | 🔴 | M6 | |
| GET /v1/retrieve/trace/:query_id | 🔴 | M6 | |
| POST /v1/workspaces | 🔴 | M6 | |
| GET /v1/workspaces/:id | 🔴 | M6 | |
| PATCH /v1/workspaces/:id/config | 🔴 | M6 | Scoring weights, thresholds, provider config |
| POST /v1/workspaces/:id/integrations | 🔴 | M6 | |
| GET /v1/workspaces/:id/integrations | 🔴 | M6 | With health status |
| DELETE /v1/workspaces/:id/integrations/:source | 🔴 | M6 | |
| POST /v1/workspaces/:id/keys | 🔴 | M6 | API key creation |
| GET /v1/workspaces/:id/keys | 🔴 | M6 | |
| DELETE /v1/workspaces/:id/keys/:key_id | 🔴 | M6 | Revoke |
| GET /v1/workspaces/:id/audit | 🔴 | M6 | Paginated audit log |
| GET /v1/workspaces/:id/dlq | 🔴 | M6 | |
| POST /v1/workspaces/:id/dlq/:job_id/retry | 🔴 | M6 | DLQ retry |
| DELETE /v1/workspaces/:id/dlq/:job_id | 🔴 | M6 | Discard |
| GET /v1/workspaces/:id/export | 🔴 | M7 | JSONL streaming export |

---

## Auth & Security

| Feature | Status | Milestone | Notes |
|---------|--------|-----------|-------|
| API key auth (X-API-Key header) | 🔴 | M6 | |
| Argon2id key hashing | 🔴 | M6 | Plaintext returned once on creation |
| Key revocation | 🔴 | M6 | |
| Rate limiting (per workspace, per endpoint group) | 🔴 | M6 | Redis sliding window |
| 429 response with Retry-After header | 🔴 | M6 | |

---

## Audit & Observability

| Feature | Status | Milestone | Notes |
|---------|--------|-----------|-------|
| Audit log (all state-changing operations) | 🔴 | M6 | AuditEntry with before/after diff |
| tracing + OpenTelemetry instrumentation | 🔴 | M6 | All services |
| Integration health dashboard data | 🔴 | M6 | |
| Ingestion metrics (events/min per source) | 🔴 | M6 | |

---

## Frontend — Memory Control Center

| Feature | Status | Milestone | Notes |
|---------|--------|-----------|-------|
| Project scaffold (Vite + React 19 + TS + Tailwind v4 + shadcn) | 🔴 | M5 | |
| Zustand store (workspaceId + apiKey in-memory only) | 🔴 | M5 | |
| TanStack Query API client wired to live endpoints | 🔴 | M5 | |
| Memory Explorer (search, filter, sort, paginate) | 🔴 | M5 | `GET /v1/memory`, `POST /v1/memory/search` |
| Memory Detail view (entities, scope, score, tags) | 🔴 | M5 | `GET /v1/memory/:id` |
| Pin / Tag / Importance override actions | 🔴 | M5 | `PATCH /v1/memory/:id` |
| Webhook tester (fire real GitHub payloads at /v1/ingest/github) | 🔴 | M5 | |
| Settings view (workspace config display, read-only) | 🔴 | M5 | Wired to real data in M6 |
| Retrieval Trace view (stubbed, empty state) | 🔴 | M5 | Wired in M6 |
| Lifecycle / Promotion Timeline (stubbed) | 🔴 | M5 | Wired in M8 |
| Integration Status view (stubbed) | 🔴 | M5 | Wired in M6 |
| Audit Log view (stubbed) | 🔴 | M5 | Wired in M6 |
| Bulk pin / bulk delete | 🔴 | M6 | |
| Memory merge UI | 🔴 | M6 | |
| Export trigger (download JSONL) | 🔴 | M7 | |

---

## Lifecycle Management

| Feature | Status | Milestone | Notes |
|---------|--------|-----------|-------|
| Decay scoring (daily scheduler) | 🔴 | M6 | Configurable per memory type |
| Soft delete / archive at decay threshold | 🔴 | M6 | |
| 30-day recovery window | 🔴 | M6 | |
| Hard delete after recovery window | 🔴 | M6 | |
| Pin = decay freeze | 🟢 | M4 | Implemented: pinned skipped in decay pass |
| Manual importance override | 🟢 | M4 | Stored, used in scoring, skips decay |
| Memory version history | 🔴 | M6 | Incremented on edit |

---

## Export & Backup

| Feature | Status | Milestone | Notes |
|---------|--------|-----------|-------|
| JSONL export (streaming) | 🔴 | M7 | Memory units only, no raw payloads |
| Import (restore) | 🔴 | v0.3 | Not in v0.1 scope |
