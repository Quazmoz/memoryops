# MemoryOps — Feature List

**Version:** 0.2.0  
**Last Updated:** 2026-04-12

This document tracks all planned features across the platform with current status and target milestone.

---

## Status Key

| Symbol | Meaning |
|--------|---------|
| 🔴 | Not started |
| 🟡 | In design |
| 🟢 | Complete |

---

## Ingestion

| Feature | Status | Milestone | Notes |
|---------|--------|-----------|-------|
| GitHub webhook receiver | 🔴 | M2 | HMAC-SHA256 validation |
| GitHub event normalization (PR, push, review, issue) | 🔴 | M2 | Maps to RawEvent |
| Slack webhook receiver | 🔴 | M9 | message, thread, reaction |
| HMAC signature validation (shared WebhookValidator trait) | 🔴 | M2 | GitHub + Slack via same interface |
| RawEvent Postgres write (transactional) | 🔴 | M2 | |
| Redis job enqueue on ingest | 🔴 | M2 | |
| Integration health tracking (last_event_at, error_count) | 🔴 | M7 | |
| Dead letter queue for failed jobs | 🔴 | M7 | Redis list |
| DLQ manual retry API | 🔴 | M7 | POST /workspaces/:id/dlq/:job_id/retry |
| Auto-retry with exponential backoff (max 3) | 🔴 | M7 | |
| Ingestion rate metrics | 🔴 | M7 | Per workspace, per source |

---

## Processing

| Feature | Status | Milestone | Notes |
|---------|--------|-----------|-------|
| Fast path worker (no LLM) | 🔴 | M3 | Entity extraction, tagging, importance scoring |
| Entity extraction (regex + rules) | 🔴 | M3 | Person, Repo, Branch, File, Team |
| Importance scoring (rule table) | 🔴 | M3 | Workspace-configurable thresholds |
| Episodic MemoryUnit creation | 🔴 | M3 | |
| Slow path async worker | 🔴 | M4 | Consumes Redis queue |
| LLM summarization (OllamaProvider default) | 🔴 | M4 | Via LlmProvider trait |
| Embedding generation (fastembed-rs default) | 🔴 | M4 | Via EmbeddingProvider trait |
| Qdrant vector write | 🔴 | M4 | |
| Promotion pipeline (episodic → semantic) | 🔴 | M5 | Cluster → threshold → summarize → promote |
| Configurable promotion threshold | 🔴 | M5 | Per workspace |
| Configurable clustering time window | 🔴 | M5 | Per workspace |
| Memory deduplication | 🔴 | M5 | Cosine similarity threshold |
| Decay scheduler (daily job) | 🔴 | M7 | Configurable decay rates |
| Pruning (soft delete at decay < 0.1) | 🔴 | M7 | |

---

## AI Providers (Pluggable)

| Feature | Status | Milestone | Notes |
|---------|--------|-----------|-------|
| EmbeddingProvider trait | 🔴 | M4 | In common crate |
| FastEmbedProvider (local, default) | 🔴 | M4 | fastembed-rs |
| OpenAIEmbedProvider | 🔴 | M4 | text-embedding-3-small |
| LlmProvider trait | 🔴 | M4 | In common crate |
| OllamaProvider (local, default) | 🔴 | M4 | Configurable model |
| OpenAIProvider | 🔴 | M4 | Chat Completions API |
| AnthropicProvider | 🔴 | M4 | Messages API |
| Provider selection via TOML config | 🔴 | M4 | No code changes to swap |

---

## Retrieval Engine

| Feature | Status | Milestone | Notes |
|---------|--------|-----------|-------|
| Hybrid search (Qdrant + Tantivy) | 🔴 | M6 | Top 50 candidates |
| Scoring formula (5 components) | 🔴 | M6 | Workspace-configurable weights |
| Token packing (greedy, under budget) | 🔴 | M6 | |
| Deduplication in packing (cosine > 0.92) | 🔴 | M6 | |
| Scope-aware retrieval (agent/user/repo) | 🔴 | M6 | Narrows to MemoryScope |
| Retrieval trace generation | 🔴 | M6 | Per-component scores, exclusion reasons |
| Trace persistence (30 days) | 🔴 | M6 | Queryable by query_id |
| Configurable scoring weights per workspace | 🔴 | M7 | |
| Configurable source authority per source | 🔴 | M7 | |

---

## API

| Feature | Status | Milestone | Notes |
|---------|--------|-----------|-------|
| POST /retrieve | 🔴 | M6 | Core retrieval endpoint |
| GET /retrieve/trace/:query_id | 🔴 | M6 | |
| GET /memory | 🔴 | M7 | Paginated, filterable |
| GET /memory/:id | 🔴 | M7 | |
| PATCH /memory/:id | 🔴 | M7 | Pin, tag, edit, importance override |
| DELETE /memory/:id | 🔴 | M7 | Soft delete |
| POST /memory/:id/promote | 🔴 | M7 | |
| POST /memory/bulk | 🔴 | M7 | Bulk pin / bulk delete |
| GET /memory/:id/history | 🔴 | M7 | Version history |
| POST /memory/merge | 🔴 | M7 | Merge two semantic memories |
| POST /workspaces | 🔴 | M7 | |
| GET /workspaces/:id | 🔴 | M7 | |
| PATCH /workspaces/:id/config | 🔴 | M7 | Scoring weights, thresholds, provider config |
| POST /workspaces/:id/integrations | 🔴 | M7 | |
| GET /workspaces/:id/integrations | 🔴 | M7 | With health status |
| DELETE /workspaces/:id/integrations/:source | 🔴 | M7 | |
| POST /workspaces/:id/keys | 🔴 | M7 | API key creation |
| GET /workspaces/:id/keys | 🔴 | M7 | |
| DELETE /workspaces/:id/keys/:key_id | 🔴 | M7 | Revoke |
| GET /workspaces/:id/audit | 🔴 | M7 | Paginated audit log |
| GET /workspaces/:id/export | 🔴 | M8 | JSONL streaming export |
| POST /workspaces/:id/dlq/:job_id/retry | 🔴 | M7 | DLQ retry |

---

## Auth & Security

| Feature | Status | Milestone | Notes |
|---------|--------|-----------|-------|
| API key auth (X-API-Key header) | 🔴 | M7 | |
| Argon2id key hashing | 🔴 | M7 | Plaintext returned once on creation |
| Key revocation | 🔴 | M7 | |
| Rate limiting (per workspace, per endpoint group) | 🔴 | M7 | Redis sliding window |
| 429 response with Retry-After header | 🔴 | M7 | |

---

## Audit & Observability

| Feature | Status | Milestone | Notes |
|---------|--------|-----------|-------|
| Audit log (all state-changing operations) | 🔴 | M7 | AuditEntry with before/after diff |
| tracing + OpenTelemetry instrumentation | 🔴 | M7 | All services |
| Integration health dashboard data | 🔴 | M7 | |
| Ingestion metrics (events/min per source) | 🔴 | M7 | |

---

## Frontend — Memory Control Center

| Feature | Status | Milestone | Notes |
|---------|--------|-----------|-------|
| Memory Explorer (search, filter) | 🔴 | M8 | Filter by scope/type/source/tag/entity |
| Memory Detail view | 🔴 | M8 | Entities, score breakdown, lineage, version history |
| Retrieval Trace view | 🔴 | M8 | Included/excluded + per-component scores |
| Lifecycle / Promotion Timeline | 🔴 | M8 | Episodic → semantic visualization |
| Integration Status view | 🔴 | M8 | Health, ingestion rate, DLQ |
| Audit Log view | 🔴 | M8 | |
| Pin / Delete / Promote / Edit actions | 🔴 | M8 | |
| Importance score override | 🔴 | M8 | |
| Bulk pin / bulk delete | 🔴 | M8 | |
| Memory merge UI | 🔴 | M8 | |
| Export trigger (download JSONL) | 🔴 | M8 | |

---

## Lifecycle Management

| Feature | Status | Milestone | Notes |
|---------|--------|-----------|-------|
| Decay scoring (daily scheduler) | 🔴 | M7 | Configurable per memory type |
| Soft delete / archive at decay threshold | 🔴 | M7 | |
| 30-day recovery window | 🔴 | M7 | |
| Hard delete after recovery window | 🔴 | M7 | |
| Pin = decay freeze | 🔴 | M7 | |
| Manual importance override | 🔴 | M7 | Stored, used in scoring |
| Memory version history | 🔴 | M7 | Incremented on edit |

---

## Export & Backup

| Feature | Status | Milestone | Notes |
|---------|--------|-----------|-------|
| JSONL export (streaming) | 🔴 | M8 | Memory units only, no raw payloads |
| Import (restore) | 🔴 | v0.3 | Not in v0.1 scope |
