# MemoryOps — Feature List

**Version:** 0.15.0
**Last Updated:** 2026-04-28

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
| M9 | Slack ingestion | 🟢 Complete |
| M10 | Linear + Jira ingestion | 🟢 Complete |
| M11 | MCP server | 🟢 Complete |
| M12 | Lifecycle configuration | 🟢 Complete |
| M13 | Workspace stats endpoint + Dashboard KPIs | 🟢 Complete |
| M14 | Retrieval Trace view (live data) | 🟢 Complete |
| M15 | Webhook Tester — multi-source (Slack, Linear, Jira) | 🟢 Complete |
| M16 | DLQ management UI (retry + discard from the Integration view) | 🟢 Complete |
| M17 | Settings view — write mode (config sliders + provider selection) | 🟢 Complete |
| M18 | Workspace stats time-series endpoint + trend charts | 🟢 Complete |
| M19 | Scope-filtered retrieval (agent_id, user_id, repo on search + retrieve) | 🟢 Complete |
| M20 | Tag management API + UI (enumerate, count, bulk-retag) | 🟢 Complete |
| M21 | Import / restore (JSONL round-trip with export) | 🟢 Complete |
| M22 | Metrics dashboard (OTel summary endpoint + UI panel) | 🟢 Complete |
| M23 | Property-based tests for token packing (proptest) | 🟢 Complete |
| M24 | Playwright E2E test suite | 🔴 Not started |
| M25 | HTTP Skills (agent-callable skill registry) | 🔴 Not started |
| M26 | Contradiction detection | 🔴 Not started |
| M27 | Provenance graph + lineage API | 🔴 Not started |
| M28 | Point-in-time memory queries | 🔴 Not started |
| M29 | Multi-agent scope inheritance + publish | 🔴 Not started |
| M30 | Memory health score + drift alerts | 🔴 Not started |
| M31 | Compliance mode (retention + right-to-erasure) | 🔴 Not started |
| M32 | Agent-authored observation ingest | 🔴 Not started |
| M33 | LOCOMO benchmark integration | 🔴 Not started |

---

## Scaffold & Infrastructure

| Feature | Status | Milestone | Notes |
|---------|--------|-----------|-------|
| Cargo workspace root | 🟢 | M1 | 5 crates wired |
| docker-compose (dev) | 🟢 | M1 | Postgres, Redis, Qdrant |
| docker-compose.test.yml | 🟢 | M1 | Isolated test infra |
| sqlx migrations scaffold | 🟢 | M1 | 0001–0014 applied |
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
| Slack webhook receiver | 🟢 | M9 | Slack Events API receiver at POST /v1/ingest/slack |
| Slack message ingestion | 🟢 | M9 | message, message.edited, app_mention |
| Slack reaction ingestion | 🟢 | M9 | reaction_added with channel + message timestamp lineage |
| Linear webhook receiver | 🟢 | M10 | X-Linear-Signature HMAC-SHA256 |
| Linear event normalization | 🟢 | M10 | Issue, Comment, Project, Cycle → RawEvent |
| Jira webhook receiver | 🟢 | M10 | X-Hub-Signature HMAC-SHA256 |
| Jira event normalization | 🟢 | M10 | issue_created/updated/deleted, comment → RawEvent |

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
| Per-workspace decay half-life | 🟢 | M12 | decay_half_life_days in WorkspaceConfig |
| Configurable pruning threshold | 🟢 | M12 | soft-delete threshold per workspace |

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
| GET /v1/workspaces/:id/stats | 🟢 | M13 | Aggregate memory stats per workspace |
| GET /v1/workspaces/:id/stats/history | 🟢 | M18 | Daily aggregate: created, promoted, soft-deleted per day |
| GET /v1/workspaces/:id/tags | 🟢 | M20 | Tag name + count enumeration |
| POST /v1/workspaces/:id/import | 🟢 | M21 | JSONL import; idempotent on id |

---

## MCP Server

| Feature | Status | Milestone | Notes |
|---------|--------|-----------|-------|
| MCP server crate scaffold | 🟢 | M11 | new crate: crates/mcp/ |
| memory_retrieve tool | 🟢 | M11 | in-process retrieval with token packing |
| memory_search tool | 🟢 | M11 | in-process memory search |
| memory_store tool | 🟢 | M11 | stores an episodic MemoryUnit and enqueues slow-path processing |
| stdio + HTTP SSE transports | 🟢 | M11 | MCP 2025-06-18 spec |
| MCP endpoint in docker-compose | 🟢 | M11 | profile-gated service on port 3003 |

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
| In-process metrics registry (counters + histograms) | 🟢 | M22 | crates/common::telemetry |
| Metrics summary endpoint | 🟢 | M22 | GET /v1/workspaces/:id/metrics |
| Ingest events / slow-path / retrieval counters | 🟢 | M22 | Recorded at handler + worker call sites |
| Embedding + LLM latency histograms (p50/p99) | 🟢 | M22 | Recorded around provider calls |
| Token-pack budget-used histogram (mean) | 🟢 | M22 | Recorded after pack_memories |

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
| Retrieval Trace view (live query + trace drill-down) | 🟢 | M14 | Replaces M5 stub |
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
| Dashboard KPI strip (6 cards via /stats) | 🟢 | M13 | Replaces 3 useMemoryList calls |
| Dashboard secondary stats row | 🟢 | M13 | Memory health + 30-day activity cards |
| Retrieval Trace view (live data) | 🔴 | M14 | GET /v1/retrieve/trace/:query_id; per-component score breakdown |
| Webhook Tester — Slack fixtures | 🟢 | M15 | POST /v1/ingest/slack; source tab switcher |
| Webhook Tester — Linear fixtures | 🟢 | M15 | POST /v1/ingest/linear |
| Webhook Tester — Jira fixtures | 🟢 | M15 | POST /v1/ingest/jira |
| DLQ retry action (from Integration view) | 🟢 | M16 | POST /v1/workspaces/:id/dlq/:job_id/retry |
| DLQ discard action (from Integration view) | 🟢 | M16 | DELETE /v1/workspaces/:id/dlq/:job_id |
| DLQ error detail expand (raw payload + error message) | 🟢 | M16 | Inline expandable row in DLQ panel |
| Settings view — config write (decay, pruning, promotion sliders) | 🟢 | M17 | PATCH /v1/workspaces/:id/config |
| Settings view — provider selector (LLM + embedding) | 🟢 | M17 | Dropdown saved via PATCH /v1/workspaces/:id/config |
| Dashboard trend charts (30-day memory activity) | 🟢 | M18 | GET /v1/workspaces/:id/stats/history |
| Tag browser (enumerate + filter by tag) | 🟢 | M20 | GET /v1/workspaces/:id/tags |
| Dashboard Metrics panel (3×3 telemetry grid, auto-refresh 30s) | 🟢 | M22 | useMetrics hook → /v1/workspaces/:id/metrics |

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
| Import (restore) | 🟢 | M21 | JSONL import endpoint available in v0.x |
| JSONL import (restore from export) | 🟢 | M21 | POST /v1/workspaces/:id/import; idempotent on id |

---

## Retrieval — Scope Filtering

| Feature | Status | Milestone | Notes |
|---------|--------|-----------|-------|
| agent_id filter on POST /v1/memory/search | 🟢 | M19 | Narrow results to a single agent |
| user_id filter on POST /v1/memory/search | 🟢 | M19 | |
| repo filter on POST /v1/memory/search | 🟢 | M19 | |
| agent_id filter on POST /v1/retrieve | 🟢 | M19 | |
| user_id filter on POST /v1/retrieve | 🟢 | M19 | |
| repo filter on POST /v1/retrieve | 🟢 | M19 | |

---

## Testing

| Feature | Status | Milestone | Notes |
|---------|--------|-----------|-------|
| proptest token packing invariants | 🟢 | M23 | packed ≤ budget; dedup threshold respected; all excluded in trace |
| Playwright E2E — ingest → process → search flow | 🔴 | M24 | |
| Playwright E2E — promotion via repeated access | 🔴 | M24 | |
| Playwright E2E — DLQ retry flow | 🔴 | M24 | |

---

## Future / v1.0

| Feature | Status | Milestone | Notes |
|---------|--------|-----------|-------|
| HTTP Skills (agent-callable skill registry) | 🔴 | M25 | |

---

## Planned Features (M26–M33)

| Feature | Status | Milestone | Notes |
|---------|--------|-----------|-------|
| Contradiction detection — semantic conflict check on new ingest | 🔴 | M26 | Flags conflicts in audit log; configurable: auto-resolve (newer wins) or quarantine |
| Contradiction quarantine review API + UI | 🔴 | M26 | GET /v1/workspaces/:id/contradictions; resolve/dismiss actions |
| Provenance graph — GET /v1/memory/:id/provenance | 🔴 | M27 | Returns DAG: source events → episodic → semantic → merges → accesses |
| Provenance tree in MemoryDetail UI | 🔴 | M27 | Visual lineage panel in frontend MemoryDetail view |
| Point-in-time retrieval — as_of param on GET /v1/memory | 🔴 | M28 | Reconstructs memory state at timestamp; uses memory_versions + decay math |
| as_of support on POST /v1/retrieve | 🔴 | M28 | Historical context retrieval for incident post-mortems |
| Multi-agent scope visibility field (private \| workspace) | 🔴 | M29 | scope.visibility on MemoryUnit |
| POST /v1/memory/:id/publish (agent → workspace pool) | 🔴 | M29 | Promoted semantic memories shared across agents |
| Sub-agent pool subscription via workspace config | 🔴 | M29 | Team agents inherit from configured sub-agent pools |
| Memory health score (0–100 composite) | 🔴 | M30 | Decay distribution, promotion rate, DLQ rate, dedup collision rate |
| GET /v1/workspaces/:id/health | 🔴 | M30 | Returns score + component breakdown |
| Health score dashboard card + trend alert | 🔴 | M30 | Redis-backed threshold; webhook notification on degradation |
| Per-workspace retention policy (max age + auto hard-purge) | 🔴 | M31 | Configured via WorkspaceConfig |
| DELETE /v1/workspaces/:id/forget/user/:user_id | 🔴 | M31 | Hard-purges all memories with matching scope.user_id |
| Compliance deletion audit trail | 🔴 | M31 | Separate compliance_audit_log table; GDPR/CCPA ready |
| POST /v1/ingest/observation (agent-authored memories) | 🔴 | M32 | Authenticated by workspace API key; same slow-path pipeline |
| MCP memory_store routes through /v1/ingest/observation | 🔴 | M32 | Closes agent read/write loop |
| cargo bench LOCOMO evaluation suite | 🔴 | M33 | Retrieval quality benchmarks against LOCOMO dataset |
| Per-workspace LOCOMO score on dashboard | 🔴 | M33 | Compare across workspace configs |

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
| 0013_slack.sql | Slack signing secret + channel/thread metadata + channel index | 🟢 M9 |
| 0014_linear_jira.sql | Linear/Jira signing secret support + active integration indexes | 🟢 M10 |
| 0015_scope_indexes.sql | Scope-filter generated columns + composite active-memory index | 🟢 M19 |
