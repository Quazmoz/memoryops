# MemoryOps — Technical Specification

**Version:** 0.2.0  
**Status:** Draft  
**Last Updated:** 2026-04-12

---

## 1. Overview

MemoryOps is a Memory Operations Platform designed to give AI agents persistent, structured, and controllable memory. It ingests raw engineering activity from external tools, transforms that activity into typed memory units, and serves optimized context back to agents at query time.

The core abstraction shift: **from storage to control**.

MemoryOps is not:
- A vector database
- A RAG wrapper
- An agent framework

MemoryOps is:
- The control plane for what AI agents remember

---

## 2. Problem Statement

| Problem | Root Cause | Impact |
|---------|-----------|--------|
| Session amnesia | No persistence layer | Agents re-ask for context every session |
| Context bloat | Naive prompt stuffing | Token waste, degraded response quality |
| Poor retrieval | Top-K only, no scoring | Irrelevant context pollutes agent output |
| No governance | No lifecycle management | Stale/wrong memories silently affect behavior |
| No debuggability | Black-box retrieval | Can't explain why agent answered incorrectly |

---

## 3. Target Users (ICP)

**Primary:** AI-native engineering teams building:
- Coding agents / dev copilots
- DevOps / SRE agents
- Internal engineering assistants

**Why this segment:**
- Clear, measurable pain
- Standardized tool integrations (GitHub, Slack, Jira)
- Technical users who can self-integrate
- ROI is easy to demonstrate

---

## 4. Design Decisions (Locked)

| # | Decision | Choice |
|---|----------|--------|
| 1 | Embedding model | Pluggable `EmbeddingProvider` trait; `fastembed-rs` local default |
| 2 | LLM summarization | Pluggable `LlmProvider` trait; Ollama local default |
| 3 | Authentication | API key per workspace (header: `X-API-Key`) |
| 4 | Retrieval mode | Pull — agents call `POST /retrieve` |
| 5 | Memory scope | Configurable hierarchy: workspace → agent → user → repo |
| 6 | Webhook validation | HMAC-SHA256 for all sources via shared `WebhookValidator` trait |

---

## 5. Core Data Model

### 5.1 RawEvent

Immutable. Written on ingestion, never mutated.

```rust
pub struct RawEvent {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub source: Source,             // GitHub | Slack | Jira | Linear
    pub event_type: EventType,      // PR | Commit | Message | Review | Issue
    pub actor: String,
    pub payload: serde_json::Value, // full raw payload preserved
    pub occurred_at: DateTime<Utc>,
    pub ingested_at: DateTime<Utc>,
}
```

### 5.2 MemoryUnit

Core product object. Created by the processor from one or more RawEvents.

```rust
pub struct MemoryUnit {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub scope: MemoryScope,               // see §5.4
    pub memory_type: MemoryType,          // Episodic | Semantic
    pub content: String,
    pub entities: Vec<Entity>,
    pub importance_score: f32,            // 0.0–1.0, user-overridable
    pub source_events: Vec<Uuid>,         // lineage to RawEvents
    pub embedding: Option<Vec<f32>>,
    pub created_at: DateTime<Utc>,
    pub last_accessed_at: Option<DateTime<Utc>>,
    pub decay_score: f32,                 // 1.0 on create; pruned at <0.1
    pub pinned: bool,                     // exempt from decay
    pub tags: Vec<String>,
    pub version: i32,                     // incremented on edit
}
```

### 5.3 Memory Types

| Type | Description | Mutability | Example |
|------|-------------|------------|---------|
| `Episodic` | Raw time-indexed event | Immutable | "PR #42 opened by alice at 14:32" |
| `Semantic` | Distilled knowledge | Updateable (versioned) | "Alice owns the auth module" |

### 5.4 Memory Scope

Scope is configurable at workspace level. Operators define which scope dimensions are active.

```rust
pub struct MemoryScope {
    pub workspace_id: Uuid,
    pub agent_id: Option<String>,   // e.g. "code-reviewer", "deploy-bot"
    pub user_id: Option<String>,    // e.g. GitHub login
    pub repo: Option<String>,       // e.g. "org/repo-name"
}
```

Retrieval is automatically scoped to the narrowest matching scope for the requesting agent.

### 5.5 Entity

```rust
pub struct Entity {
    pub entity_type: EntityType, // Person | Repo | Branch | Topic | File | Team
    pub value: String,
    pub confidence: f32,
}
```

---

## 6. Provider Traits (Pluggable AI)

All AI integrations are abstracted behind async traits. Configuration selects the active provider.

### 6.1 EmbeddingProvider

```rust
#[async_trait]
pub trait EmbeddingProvider: Send + Sync {
    async fn embed(&self, text: &str) -> Result<Vec<f32>>;
    async fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>>;
    fn dimensions(&self) -> usize;
    fn model_name(&self) -> &str;
}
```

**Implementations (v0.1):**
- `FastEmbedProvider` — local, uses `fastembed-rs`, no network calls, default
- `OpenAIEmbedProvider` — `text-embedding-3-small`, requires `OPENAI_API_KEY`

### 6.2 LlmProvider

```rust
#[async_trait]
pub trait LlmProvider: Send + Sync {
    async fn complete(&self, prompt: &str) -> Result<String>;
    async fn summarize(&self, text: &str, max_tokens: usize) -> Result<String>;
}
```

**Implementations (v0.1):**
- `OllamaProvider` — calls local Ollama HTTP API, model configurable (default: `llama3`)
- `OpenAIProvider` — calls OpenAI Chat Completions API
- `AnthropicProvider` — calls Anthropic Messages API

### 6.3 Provider Config (TOML)

```toml
[embedding]
provider = "fastembed"      # fastembed | openai
model = "BAAI/bge-small-en-v1.5"

[llm]
provider = "ollama"         # ollama | openai | anthropic
model = "llama3"
base_url = "http://localhost:11434"
```

---

## 7. Authentication

- API key per workspace, passed as `X-API-Key` header
- Keys are hashed (Argon2id) before storage — never stored in plaintext
- Key creation returns plaintext once; not recoverable
- Rate limiting enforced per workspace per endpoint (configurable)
- Key rotation supported: create new → verify → revoke old

```rust
pub struct ApiKey {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub name: String,           // human label
    pub key_hash: String,       // Argon2id hash
    pub created_at: DateTime<Utc>,
    pub last_used_at: Option<DateTime<Utc>>,
    pub revoked: bool,
}
```

---

## 8. System Architecture

### 8.1 Services

```
┌───────────────────────────────────────────────────────────┐
│                        MemoryOps                          │
│                                                           │
│  ┌─────────────────┐   ┌──────────────────────────────┐  │
│  │  Ingestion Svc  │   │       Processor Svc          │  │
│  │  POST /webhook  │──▶│  Fast Path  │  Slow Path     │  │
│  │  /github /slack │   │  (sync)     │  (async/LLM)   │  │
│  └─────────────────┘   └──────────────────────────────┘  │
│           │                       │                       │
│           ▼                       ▼                       │
│       Postgres               Redis Queue                  │
│       raw_events             processor_jobs               │
│       memory_units                                        │
│       audit_log                                           │
│           │                       │                       │
│           └──────────┬────────────┘                       │
│                      ▼                                    │
│             ┌─────────────────┐                           │
│             │  Retrieval Svc  │                           │
│             │  Qdrant (vec)   │                           │
│             │  Tantivy (BM25) │                           │
│             │  Scorer         │                           │
│             │  Token Packer   │                           │
│             └─────────────────┘                           │
│                      │                                    │
│             ┌─────────────────┐                           │
│             │    API Svc      │◀── Agent / UI requests    │
│             │    (axum)       │                           │
│             └─────────────────┘                           │
└───────────────────────────────────────────────────────────┘
```

### 8.2 Crate Layout

```
crates/
├── common/      # Types, DB pool, config, error types, provider traits
├── ingestion/   # Webhook handlers, HMAC validation, source parsers
├── processor/   # Fast path worker, slow path LLM worker, promotion logic
├── retrieval/   # Scorer, hybrid search, token packer, trace builder
└── api/         # axum router, middleware (auth, rate limit), request/response types
```

---

## 9. Ingestion Layer

### 9.1 Webhook Validation (Shared Trait)

```rust
pub trait WebhookValidator {
    fn validate(&self, payload: &[u8], signature: &str) -> Result<()>;
}
```

- `GitHubValidator` — HMAC-SHA256 of payload with `X-Hub-Signature-256` header
- `SlackValidator` — HMAC-SHA256 of `v0:timestamp:body` with `X-Slack-Signature` header  
  (Both reduce to the same HMAC-SHA256 primitive; Slack's prefix is handled in the adapter)

### 9.2 GitHub Integration

**Events handled:**
- `pull_request` — opened, closed, merged, review requested
- `pull_request_review` — submitted (approved, changes_requested, commented)
- `push` — commits with ref and author
- `issue_comment` — created, edited
- `issues` — opened, closed, labeled

### 9.3 Slack Integration (v0.2)

**Events handled:**
- `message` in configured channels
- `reaction_added` on tracked messages
- Thread replies

### 9.4 Ingestion Endpoint Behavior

1. Validate HMAC signature → `401` on failure
2. Deserialize payload
3. Write `RawEvent` to Postgres (transactional)
4. Enqueue processor job to Redis
5. Return `202 Accepted`
6. Write to audit log

### 9.5 Integration Health

Each integration tracks:
- `last_event_at` — timestamp of most recent valid webhook
- `error_count_24h` — validation/parse failures in last 24h
- `status` — `active | degraded | failing`

Dead letter queue (Redis list) captures failed jobs for manual inspection and retry.

---

## 10. Processing Pipeline

### 10.1 Fast Path (Sync, No LLM)

- Entity extraction: regex + rules (people, repos, branches, file paths, teams)
- Tag assignment based on event type + entity patterns
- Importance scoring (rule table, configurable per workspace):
  - Merge to default branch → `0.9`
  - PR review: changes requested → `0.75`
  - PR opened → `0.6`
  - Push to feature branch → `0.3`
- Write `MemoryUnit` (Episodic, no embedding)

### 10.2 Slow Path (Async, LLM-backed)

- Consumes jobs from Redis queue
- Calls `LlmProvider::summarize` on raw event content
- Calls `EmbeddingProvider::embed` on summary
- Writes embedding to Qdrant
- Triggers promotion evaluation

### 10.3 Promotion Pipeline

```
RawEvents
    │
    ▼
Episodic MemoryUnits  (fast path)
    │
    ▼
Clustering  (group by entity + topic + time window)
    │
    ▼
Importance Threshold Check  (workspace-configurable, default: score > 0.65)
    │
    ▼
LLM Summarization  (via LlmProvider)
    │
    ▼
Semantic MemoryUnit  (versioned, stable, embedded)
```

Importance threshold, clustering window, and promotion cadence are all workspace-configurable.

---

## 11. Retrieval Engine

### 11.1 Retrieval Request

```rust
pub struct RetrievalRequest {
    pub workspace_id: Uuid,
    pub scope: Option<MemoryScope>,     // narrows to agent/user/repo if provided
    pub query: String,
    pub token_budget: usize,
    pub filters: Option<RetrievalFilters>,
}

pub struct RetrievalFilters {
    pub sources: Option<Vec<Source>>,
    pub memory_types: Option<Vec<MemoryType>>,
    pub entities: Option<Vec<String>>,
    pub since: Option<DateTime<Utc>>,
    pub tags: Option<Vec<String>>,
}
```

### 11.2 Scoring Formula

```
final_score =
    (semantic_similarity  × 0.35)
  + (importance_score     × 0.25)
  + (recency_score        × 0.20)
  + (source_authority     × 0.10)
  + (memory_type_weight   × 0.10)
```

Weights are workspace-configurable. Defaults shown above.

- `recency_score` = `1 / (1 + days_since_event)`
- `source_authority` = per-source float, workspace config (default: GitHub `0.9`, Slack `0.6`)
- `memory_type_weight` = Semantic `1.0`, Episodic `0.5`

### 11.3 Token Packing

```
1. Hybrid candidate fetch: Qdrant (semantic) + Tantivy (BM25), top 50
2. Score all candidates
3. Sort descending by final_score
4. Greedy pack under token_budget
5. Dedup: skip if cosine_similarity > 0.92 to any already-included item
6. Return RetrievalResult with full trace
```

### 11.4 Retrieval Response

```rust
pub struct RetrievalResult {
    pub query_id: Uuid,                     // trace is persisted by this ID
    pub memories: Vec<ScoredMemory>,
    pub total_tokens: usize,
    pub trace: Vec<RetrievalTraceEntry>,
    pub retrieved_at: DateTime<Utc>,
}

pub struct RetrievalTraceEntry {
    pub memory_id: Uuid,
    pub final_score: f32,
    pub score_breakdown: ScoreBreakdown,    // per-component scores
    pub token_count: usize,
    pub included: bool,
    pub exclusion_reason: Option<String>,   // "token_budget" | "dedup" | "filtered"
}
```

Retrieval traces are persisted for 30 days and queryable via `GET /retrieve/trace/:query_id`.

---

## 12. API Surface

### 12.1 Ingestion

| Method | Path | Description |
|--------|------|-------------|
| POST | `/webhooks/github` | GitHub webhook receiver |
| POST | `/webhooks/slack` | Slack Events API receiver |

### 12.2 Memory Management

| Method | Path | Description |
|--------|------|-------------|
| GET | `/memory` | List memory units (paginated, filterable by scope/type/source/tag) |
| GET | `/memory/:id` | Get single memory unit with full entity + lineage data |
| PATCH | `/memory/:id` | Update: pin, tag, edit content, override importance score |
| DELETE | `/memory/:id` | Soft delete (recoverable for 30 days) |
| POST | `/memory/:id/promote` | Force episodic → semantic promotion |
| POST | `/memory/bulk` | Bulk pin / bulk delete by filter or ID list |
| GET | `/memory/:id/history` | Version history for semantic memories |
| POST | `/memory/merge` | Merge two semantic memory units |

### 12.3 Retrieval

| Method | Path | Description |
|--------|------|-------------|
| POST | `/retrieve` | Core retrieval — returns scored, token-packed context |
| GET | `/retrieve/trace/:query_id` | Full trace for a past retrieval query |

### 12.4 Workspace & Config

| Method | Path | Description |
|--------|------|-------------|
| POST | `/workspaces` | Create workspace |
| GET | `/workspaces/:id` | Get workspace + integration status |
| PATCH | `/workspaces/:id/config` | Update scoring weights, thresholds, provider config |
| POST | `/workspaces/:id/integrations` | Register GitHub/Slack integration |
| GET | `/workspaces/:id/integrations` | List integrations with health status |
| DELETE | `/workspaces/:id/integrations/:source` | Remove integration |

### 12.5 API Keys

| Method | Path | Description |
|--------|------|-------------|
| POST | `/workspaces/:id/keys` | Create API key (returns plaintext once) |
| GET | `/workspaces/:id/keys` | List keys (hashed, with last_used_at) |
| DELETE | `/workspaces/:id/keys/:key_id` | Revoke key |

### 12.6 Audit Log

| Method | Path | Description |
|--------|------|-------------|
| GET | `/workspaces/:id/audit` | Paginated audit log (memory edits, deletes, key events) |

---

## 13. Multi-Tenancy

- Every table includes `workspace_id UUID NOT NULL`
- All queries scoped by `workspace_id` at the `sqlx` param level — never application-layer filtering
- `MemoryScope` provides sub-workspace granularity (agent / user / repo)
- Workspace config controls active scope dimensions, scoring weights, provider selection, importance thresholds

---

## 14. Decay & Pruning

- `decay_score` starts at `1.0`, computed daily by scheduled job
- Formula: `new_decay = current_decay × decay_rate ^ days_since_last_access`
- `decay_rate` is workspace-configurable (default: `0.98` Semantic, `0.95` Episodic)
- Pinned memories: decay frozen
- Pruning threshold: `decay_score < 0.1` → soft-deleted (archived)
- Archived memories recoverable via UI for 30 days, then hard-deleted

---

## 15. Frontend — Memory Control Center

### 15.1 Views

| View | Description |
|------|-------------|
| Memory Explorer | Searchable, filterable list; filter by scope/type/source/tag/entity |
| Memory Detail | Full content, entities, score breakdown, source event lineage, version history |
| Retrieval Trace | For any past query_id: included/excluded memories, per-component scores, exclusion reasons |
| Lifecycle View | Timeline of episodic → semantic promotions with clustering visualization |
| Integration Status | Per-source webhook health, ingestion rate, error count, dead letter queue |
| Audit Log | Who edited/deleted/pinned what memory and when |

### 15.2 Memory Actions

| Action | Description |
|--------|-------------|
| Pin | Lock memory; freeze decay |
| Delete | Soft delete; recoverable 30 days |
| Merge | Combine two semantic memories into one |
| Edit | Manually correct content; creates new version |
| Promote | Force episodic → semantic |
| Override importance | Manually set importance score (0.0–1.0) |
| Bulk pin / bulk delete | Apply action to filtered set |
| Export | Download workspace memories as JSON or JSONL |

---

## 16. Audit Log

Every state-changing operation writes an audit entry:

```rust
pub struct AuditEntry {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub actor: AuditActor,         // ApiKey name or "system"
    pub action: AuditAction,       // Created | Edited | Deleted | Pinned | Promoted | Merged | KeyCreated | KeyRevoked
    pub target_id: Uuid,           // memory_id, key_id, etc.
    pub target_type: String,
    pub diff: Option<serde_json::Value>, // before/after for edits
    pub occurred_at: DateTime<Utc>,
}
```

---

## 17. Rate Limiting

- Enforced per workspace, per endpoint group
- Config in `workspace.config`:
  - `retrieve_rpm` — retrievals per minute (default: 60)
  - `ingest_rpm` — webhook events per minute (default: 300)
  - `api_rpm` — general API calls per minute (default: 120)
- Exceeding limit returns `429 Too Many Requests` with `Retry-After` header
- Implemented via Redis sliding window counter

---

## 18. Integration Health & Dead Letter Queue

- Failed webhook jobs (parse error, DB write failure) move to Redis DLQ
- DLQ entries include: original payload, failure reason, timestamp, retry count
- Manual retry available via API: `POST /workspaces/:id/dlq/:job_id/retry`
- Auto-retry: up to 3 attempts with exponential backoff before DLQ
- Integration `status` field: `active` (0 errors) | `degraded` (>5% error rate) | `failing` (>50% error rate)

---

## 19. Export & Backup

- `GET /workspaces/:id/export` — export all memory units as JSONL (streaming)
- Includes: content, entities, scores, tags, scope, source lineage
- Does not include raw event payloads (may contain PII)
- Import endpoint planned for v0.3

---

## 20. Non-Goals (v0.1)

- Generic "second brain" or consumer use case
- Full agent framework or runtime
- Vector DB replacement
- Real-time streaming ingestion
- Custom embedding model training
- Push-mode context injection (agent SDK — future)
- SSO / OAuth login (future SaaS phase)

---

## 21. Risks & Mitigations

| Risk | Mitigation |
|------|------------|
| LLM cost/latency | Async-only slow path; local Ollama default eliminates cost |
| Large context windows reducing retrieval need | Focus moat on lifecycle + governance + control UI |
| Retrieval commoditization | Differentiator is ingestion + lifecycle + trace, not raw search |
| Integration complexity | Ship GitHub only in v0.1; Slack in v0.2 |
| Vector DB space crowding | Explicitly not a vector DB — use Qdrant as infra, not product |

---

## 22. Milestones

| Milestone | Deliverable | Target |
|-----------|-------------|--------|
| M1 | Rust workspace + Cargo.toml + docker-compose + migrations scaffold | Week 1 |
| M2 | GitHub webhook ingestion + RawEvent writes to Postgres | Week 2 |
| M3 | Fast path processor + episodic MemoryUnit creation | Week 3 |
| M4 | Slow path worker + Ollama summarization + fastembed embeddings + Qdrant writes | Week 4 |
| M5 | Promotion pipeline (cluster → semantic) | Week 5 |
| M6 | Retrieval engine: scoring + token packing + trace | Week 6 |
| M7 | Full REST API + auth (API keys) + rate limiting + audit log | Week 7 |
| M8 | React Memory Control Center: explorer + detail + trace view | Week 8 |
| M9 | Slack ingestion | Week 9 |
| M10 | Demo: agent failure → memory fix → retrieval trace walkthrough | Week 10 |
