# MemoryOps — Technical Specification

**Version:** 0.10.0  
**Status:** Active  
**Last Updated:** 2026-04-28

---

## Table of Contents

1. [Overview](#1-overview)
2. [Problem Statement](#2-problem-statement)
3. [Target Users](#3-target-users)
4. [Design Decisions](#4-design-decisions)
5. [Core Data Model](#5-core-data-model)
6. [Provider Traits](#6-provider-traits)
7. [Authentication & Security](#7-authentication--security)
8. [System Architecture](#8-system-architecture)
9. [Ingestion Layer](#9-ingestion-layer)
10. [Processing Pipeline](#10-processing-pipeline)
11. [Retrieval Engine](#11-retrieval-engine)
12. [API Design](#12-api-design)
13. [Database Conventions](#13-database-conventions)
14. [Error Handling](#14-error-handling)
15. [Testing Strategy](#15-testing-strategy)
16. [Observability](#16-observability)
17. [CI/CD Pipeline](#17-cicd-pipeline)
18. [Multi-Tenancy](#18-multi-tenancy)
19. [Decay & Pruning](#19-decay--pruning)
20. [Rate Limiting](#20-rate-limiting)
21. [Frontend Architecture](#21-frontend-architecture)
22. [Audit Log](#22-audit-log)
23. [Integration Health & DLQ](#23-integration-health--dlq)
24. [MCP Server](#24-mcp-server)
25. [Export & Backup](#25-export--backup)
26. [Code Quality Standards](#26-code-quality-standards)
27. [Non-Goals](#27-non-goals)
28. [Risks & Mitigations](#28-risks--mitigations)
29. [Milestones](#29-milestones)

---

## 1. Overview

MemoryOps is a Memory Operations Platform designed to give AI agents persistent, structured, and controllable memory. It ingests raw engineering activity from external tools, transforms that activity into typed memory units, and serves optimized context back to agents at query time.

**Core abstraction shift: from storage → control.**

MemoryOps is **not**:
- A vector database
- A RAG wrapper
- An agent framework

MemoryOps **is**:
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

## 3. Target Users

**Primary ICP:** AI-native engineering teams building:
- Coding agents / dev copilots
- DevOps / SRE agents
- Internal engineering assistants

**Qualification criteria:**
- Team has an active AI agent in production or development
- Agent uses GitHub and/or Slack as primary tools
- Pain with stateless context is measurable and acknowledged

---

## 4. Design Decisions (Locked)

| # | Decision | Choice | Rationale |
|---|----------|--------|-----------|
| 1 | Embedding model | Pluggable `EmbeddingProvider` trait; `fastembed-rs` local default | No external dependency; swap via config |
| 2 | LLM summarization | Pluggable `LlmProvider` trait; Ollama local default | Self-hostable by default |
| 3 | Authentication | API key per workspace (`X-API-Key` header) | Simple, no OAuth complexity in v0.1 |
| 4 | Retrieval mode | Pull — agents call `POST /retrieve` | Clean integration contract |
| 5 | Memory scope | Configurable hierarchy: workspace → agent → user → repo | Flexible without over-engineering |
| 6 | Webhook validation | HMAC-SHA256 via shared `WebhookValidator` trait | Consistent across all sources |

---

## 5. Core Data Model

### 5.1 RawEvent

Immutable. Written on ingestion, never mutated. Source of truth for all memory lineage.

```rust
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct RawEvent {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub source: Source,
    pub event_type: EventType,
    pub actor: String,
    pub payload: serde_json::Value,
    pub occurred_at: DateTime<Utc>,
    pub ingested_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "source", rename_all = "lowercase")]
pub enum Source {
    GitHub,
    Slack,
    Jira,
    Linear,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "event_type", rename_all = "snake_case")]
pub enum EventType {
    PullRequest,
    PullRequestReview,
    Push,
    IssueComment,
    Issue,
    Message,
    Reaction,
}
```

### 5.2 MemoryUnit

Core product object. Created by the processor. Versioned on edit.

```rust
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct MemoryUnit {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub scope: MemoryScope,
    pub memory_type: MemoryType,
    pub content: String,
    pub entities: sqlx::types::Json<Vec<Entity>>,
    pub importance_score: f32,
    pub importance_overridden: bool,          // true if user manually set score
    pub source_events: Vec<Uuid>,
    pub embedding_id: Option<String>,         // Qdrant point ID
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub last_accessed_at: Option<DateTime<Utc>>,
    pub decay_score: f32,
    pub pinned: bool,
    pub tags: Vec<String>,
    pub version: i32,
    pub deleted_at: Option<DateTime<Utc>>,    // soft delete
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "memory_type", rename_all = "lowercase")]
pub enum MemoryType {
    Episodic,
    Semantic,
}
```

### 5.3 MemoryScope

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryScope {
    pub workspace_id: Uuid,
    pub agent_id: Option<String>,
    pub user_id: Option<String>,
    pub repo: Option<String>,
}

impl MemoryScope {
    /// Returns a specificity score — used to prefer narrower scopes in retrieval.
    pub fn specificity(&self) -> u8 {
        let mut score = 0u8;
        if self.agent_id.is_some() { score += 4; }
        if self.user_id.is_some()  { score += 2; }
        if self.repo.is_some()     { score += 1; }
        score
    }
}
```

### 5.4 Entity

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Entity {
    pub entity_type: EntityType,
    pub value: String,
    pub confidence: f32,  // 0.0–1.0
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EntityType {
    Person,
    Repo,
    Branch,
    Topic,
    File,
    Team,
}
```

### 5.5 MemoryVersion

Every edit to a `Semantic` memory creates a version row before mutating.

```rust
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct MemoryVersion {
    pub id: Uuid,
    pub memory_id: Uuid,
    pub workspace_id: Uuid,
    pub version: i32,
    pub content: String,
    pub importance_score: f32,
    pub tags: Vec<String>,
    pub edited_by: String,      // actor (API key name or "system")
    pub created_at: DateTime<Utc>,
}
```

---

## 6. Provider Traits

All AI integrations are behind async traits defined in `common`. No crate outside `common` imports a concrete provider directly — only the trait.

### 6.1 EmbeddingProvider

```rust
#[async_trait]
pub trait EmbeddingProvider: Send + Sync + 'static {
    async fn embed(&self, text: &str) -> Result<Vec<f32>, ProviderError>;
    async fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>, ProviderError>;
    fn dimensions(&self) -> usize;
    fn model_name(&self) -> &str;
}
```

**Implementations:**

| Provider | Crate | Notes |
|----------|-------|-------|
| `FastEmbedProvider` | `fastembed` | Local, no network, default |
| `OpenAIEmbedProvider` | `async-openai` | `text-embedding-3-small` |

### 6.2 LlmProvider

```rust
#[async_trait]
pub trait LlmProvider: Send + Sync + 'static {
    async fn complete(&self, prompt: &str) -> Result<String, ProviderError>;
    async fn summarize(
        &self,
        text: &str,
        max_tokens: usize,
    ) -> Result<String, ProviderError>;
}
```

**Implementations:**

| Provider | Notes |
|----------|-------|
| `OllamaProvider` | Local HTTP, model configurable, default |
| `OpenAIProvider` | Chat Completions API |
| `AnthropicProvider` | Messages API |

### 6.3 Provider Error Type

```rust
#[derive(Debug, thiserror::Error)]
pub enum ProviderError {
    #[error("provider request failed: {0}")]
    Request(String),
    #[error("provider rate limited, retry after {retry_after_secs}s")]
    RateLimited { retry_after_secs: u64 },
    #[error("provider returned invalid response: {0}")]
    InvalidResponse(String),
    #[error("provider not configured")]
    NotConfigured,
}
```

### 6.4 Config (TOML)

```toml
[embedding]
provider = "fastembed"           # fastembed | openai
model    = "BAAI/bge-small-en-v1.5"

[llm]
provider = "ollama"              # ollama | openai | anthropic
model    = "llama3"
base_url = "http://localhost:11434"
timeout_secs = 30

[llm.openai]                     # only read if provider = "openai"
api_key_env = "OPENAI_API_KEY"   # env var name, not value

[llm.anthropic]
api_key_env = "ANTHROPIC_API_KEY"
```

Secret values are **never** in config files — always resolved from environment variables.

---

## 7. Authentication & Security

### 7.1 API Keys

```rust
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ApiKey {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub name: String,
    pub key_hash: String,           // Argon2id hash
    pub prefix: String,             // first 8 chars of plaintext, for display
    pub created_at: DateTime<Utc>,
    pub last_used_at: Option<DateTime<Utc>>,
    pub revoked: bool,
    pub revoked_at: Option<DateTime<Utc>>,
}
```

**Key format:** `mops_<workspace_prefix>_<32 random bytes as base58>`  
**Example:** `mops_acme_3xK9mPqRvZ...`

**Hashing:** Argon2id with params: `m=65536, t=2, p=1` (OWASP recommended)

**Key lifecycle:**
1. `POST /workspaces/:id/keys` → generate plaintext, hash, store hash, return plaintext **once**
2. Client authenticates with `X-API-Key: <plaintext>`
3. On request: hash incoming key, compare to stored hash (constant-time)
4. Update `last_used_at` asynchronously (non-blocking fire-and-forget)
5. Revoke: set `revoked = true`, `revoked_at = now()`

### 7.2 Middleware Stack (axum)

Request flows through middleware in this order:

```
Request
  │
  ▼
TraceLayer          # request ID, span creation
  │
  ▼
RequestIdLayer      # inject X-Request-ID
  │
  ▼
TimeoutLayer        # 30s global timeout
  │
  ▼
CorsLayer           # configurable origins
  │
  ▼
AuthLayer           # X-API-Key → workspace_id extraction
  │                 # skipped for /v1/ingest/* and /health
  ▼
RateLimitLayer      # Redis sliding window per workspace
  │
  ▼
Handler
```

### 7.3 Security Hardening

- All DB queries use parameterized statements via `sqlx` — no string interpolation
- `workspace_id` injected from authenticated context, never from request body
- Webhook payloads validated before deserialization (fail-fast on bad HMAC)
- No secrets in logs — `tracing` fields sanitized for key material
- Dependency audit via `cargo audit` in CI
- `Content-Security-Policy`, `X-Frame-Options`, `X-Content-Type-Options` headers on all responses
- PII in raw payloads never exported (export endpoint strips `payload` field)

---

## 8. System Architecture

### 8.1 Service Overview

```
┌───────────────────────────────────────────────────────────────┐
│                         MemoryOps                             │
│                                                               │
│  ┌──────────────────┐    ┌────────────────────────────────┐   │
│  │  Ingestion Svc   │    │        Processor Svc           │   │
│  │                  │    │                                │   │
│  │ POST /v1/ingest/ │───▶│  Fast Worker  │  Slow Worker  │   │
│  │  github | slack  │    │  (sync, rules)│  (async, LLM) │   │
│  └──────────────────┘    └────────────────────────────────┘   │
│           │                          │                        │
│           ▼                          ▼                        │
│       Postgres                  Redis Streams                 │
│  ┌─────────────────┐         ┌──────────────────┐            │
│  │  raw_events     │         │  processor_jobs  │            │
│  │  memory_units   │         │  dlq             │            │
│  │  memory_versions│         └──────────────────┘            │
│  │  audit_log      │                  │                       │
│  │  api_keys       │◀─────────────────┘                       │
│  │  workspaces     │                                          │
│  └─────────────────┘                                          │
│           │                                                   │
│           ▼                                                   │
│  ┌──────────────────────────────────────┐                     │
│  │           Retrieval Svc              │                     │
│  │  Qdrant (semantic) + PG FTS (BM25)   │                     │
│  │  RRF Fusion → Decay → Promotion      │                     │
│  └──────────────────────────────────────┘                     │
│           │                                                   │
│           ▼                                                   │
│  ┌──────────────────────────────────────┐                     │
│  │             API Svc (axum)           │◀── Agents / UI      │
│  │  Auth → RateLimit → Handlers         │                     │
│  └──────────────────────────────────────┘                     │
└───────────────────────────────────────────────────────────────┘
```

### 8.2 Crate Layout

```
memoryops/
├── Cargo.toml                  # workspace root
├── crates/
│   ├── common/                 # shared across all crates
│   │   ├── src/
│   │   │   ├── config.rs       # AppConfig, loaded from TOML + env
│   │   │   ├── db.rs           # PgPool init, migration runner
│   │   │   ├── error.rs        # AppError, ProviderError, unified error types
│   │   │   ├── models/         # RawEvent, MemoryUnit, MemoryScope, Entity, ...
│   │   │   ├── providers/
│   │   │   │   ├── mod.rs
│   │   │   │   ├── traits.rs   # EmbeddingProvider, LlmProvider traits
│   │   │   │   ├── fastembed.rs
│   │   │   │   ├── ollama.rs
│   │   │   │   ├── openai.rs
│   │   │   │   └── anthropic.rs
│   │   │   └── telemetry.rs    # tracing + OTEL init
│   ├── ingestion/
│   │   ├── src/
│   │   │   ├── lib.rs
│   │   │   ├── router.rs       # axum routes for /v1/ingest/*
│   │   │   ├── github/
│   │   │   │   ├── mod.rs
│   │   │   │   ├── handler.rs
│   │   │   │   ├── signature.rs
│   │   │   │   └── parser.rs   # GitHub payload → RawEvent
│   │   │   └── slack/
│   │   │       ├── mod.rs
│   │   │       ├── handler.rs
│   │   │       ├── parser.rs
│   │   │       └── validator.rs
│   ├── processor/
│   │   ├── src/
│   │   │   ├── lib.rs
│   │   │   ├── worker.rs       # Redis consumer loop (fast + slow paths)
│   │   │   ├── extractor.rs    # entity extraction
│   │   │   ├── embedder.rs     # EmbeddingProvider + Qdrant write
│   │   │   ├── scope.rs        # MemoryScope builder
│   │   │   ├── store.rs        # DB writes for MemoryUnit
│   │   │   ├── dlq.rs          # Dead letter queue
│   │   │   └── pipeline/       # fast/slow path orchestration
│   ├── retrieval/
│   │   ├── src/
│   │   │   ├── lib.rs          # router registration
│   │   │   ├── dto.rs          # SearchRequest, ListQuery, UpdateMemoryRequest, DTOs
│   │   │   ├── handlers/       # search, list, get, update handlers
│   │   │   ├── search/         # vector, keyword, hybrid (RRF) search modules
│   │   │   ├── promotion/      # decay + eligibility + promotion trigger
│   │   │   ├── access.rs       # Redis access counter
│   │   │   └── store.rs        # all retrieval DB queries
│   └── api/
│       ├── src/
│       │   ├── main.rs         # AppState wiring, router merge, startup
│       │   ├── scheduler.rs    # API-owned scheduled background jobs
│       │   ├── middleware/
│       │   │   ├── auth.rs
│       │   │   └── rate_limit.rs
│       │   └── handlers/
│       │       ├── workspaces.rs
│       │       ├── keys.rs
│       │       └── audit.rs
├── frontend/                   # React + TypeScript (M5)
│   ├── src/
│   │   ├── components/
│   │   ├── pages/
│   │   ├── hooks/
│   │   ├── api/                # typed API client (generated from OpenAPI)
│   │   └── stores/             # Zustand state
│   └── package.json
├── migrations/                 # sqlx numbered migrations
│   ├── 0001_init.sql
│   ├── 0002_ingestion_indexes.sql
│   ├── 0003_processor.sql
│   ├── 0004_retrieval.sql      # FTS indexes, access_count column
│   ├── 0005_workspaces.sql
│   ├── 0006_api_keys.sql
│   ├── 0007_audit_log.sql
│   ├── 0008_integrations.sql
│   ├── 0009_retrieval_traces.sql
│   ├── 0010_soft_delete.sql
│   ├── 0011_scheduler.sql
│   ├── 0012_promotion.sql
│   └── 0013_slack.sql
├── docs/
│   ├── SPEC.md
│   ├── FEATURES.md
│   └── openapi.yaml            # API contract (source of truth)
├── docker-compose.yml
├── docker-compose.test.yml     # isolated test infra
└── .github/
    └── workflows/
        ├── ci.yml
        └── release.yml
```

### 8.3 AppState

All shared state is injected via axum `State<AppState>` — never global statics.

```rust
#[derive(Clone)]
pub struct AppState {
    pub db: PgPool,
    pub redis: ConnectionManager,
    pub qdrant: QdrantClient,
    pub embedding_provider: Arc<dyn EmbeddingProvider>,
    pub llm_provider: Arc<dyn LlmProvider>,
    pub config: Arc<AppConfig>,
    pub github_webhook_secret: String,
}
```

---

## 9. Ingestion Layer

### 9.1 WebhookValidator Trait

```rust
pub trait WebhookValidator: Send + Sync {
    fn validate(
        &self,
        payload: &[u8],
        headers: &HeaderMap,
    ) -> Result<(), ValidationError>;
}

#[derive(Debug, thiserror::Error)]
pub enum ValidationError {
    #[error("missing signature header")]
    MissingHeader,
    #[error("invalid signature")]
    InvalidSignature,
    #[error("timestamp too old")]  // replay attack prevention
    StaleTimestamp,
}
```

- `GitHubValidator`: verifies `X-Hub-Signature-256: sha256=<hmac>`
- `SlackValidator`: verifies `X-Slack-Signature: v0=<hmac>` with `v0:{timestamp}:{body}` message; rejects timestamps older than 5 minutes

### 9.2 GitHub Events

| Event | Actions Captured |
|-------|-----------------|
| `pull_request` | opened, closed, merged, review_requested |
| `pull_request_review` | submitted (approved, changes_requested, commented) |
| `push` | all refs; extract commits, author, branch |
| `issue_comment` | created, edited |
| `issues` | opened, closed, labeled |

### 9.3 Ingestion Flow

```
POST /v1/ingest/github
  │
  ├─ 1. Validate HMAC  →  401 ValidationError on failure
  ├─ 2. Parse event type from X-GitHub-Event header
  ├─ 3. Deserialize payload (serde_json) → 400 on parse error
  ├─ 4. Build RawEvent struct
  ├─ 5. BEGIN TRANSACTION
  │     ├─ INSERT raw_events
  │     └─ XADD processor_jobs stream
  ├─ 6. COMMIT
  ├─ 7. Return 202 Accepted { event_id }
  └─ 8. Async: write audit entry, update integration health
```

Step 5 uses a Redis pipeline inside the Postgres transaction callback to ensure the job is only enqueued if the DB write succeeds. On transaction rollback, the Redis XADD is not executed.

### 9.4 Idempotency

GitHub may deliver webhooks more than once. Idempotency key = `SHA256(source + event_type + payload.id + occurred_at)`. On conflict, return `202` without re-processing.

### 9.5 Slack Events

| Event | Meaning |
|-------|---------|
| `message` | New message in channel or thread |
| `message.edited` | Message body edited |
| `reaction_added` | Emoji reaction on a message |
| `app_mention` | Direct @mention of the MemoryOps app |

---

## 10. Processing Pipeline

### 10.1 Fast Path Worker

Runs in the same process as ingestion (tokio task), processes synchronously before returning the 202.

**Steps:**
1. Extract entities using regex patterns + allowlists
2. Assign tags from a rule table (event_type × entity_pattern → tag)
3. Score importance from rule table (workspace-configurable weights)
4. Write `MemoryUnit` (Episodic, `embedding_id = None`)
5. Enqueue slow path job

**Importance scoring rules (default):**

| Event | Default Score |
|-------|--------------|
| Merge to default branch | 0.90 |
| PR review: changes_requested | 0.75 |
| PR opened | 0.60 |
| PR review: approved | 0.55 |
| Issue opened | 0.50 |
| Issue comment | 0.35 |
| Push to feature branch | 0.30 |
| Reaction added | 0.10 |

### 10.2 Slow Path Worker

Long-running tokio task consuming from Redis Streams (`XREADGROUP`).

```
XREADGROUP GROUP slow_workers consumer-1 COUNT 10 BLOCK 2000 STREAMS processor_jobs >
  │
  ├─ For each job:
  │   ├─ Load MemoryUnit from Postgres
  │   ├─ Call LlmProvider::summarize(content)
  │   ├─ Call EmbeddingProvider::embed(summary)
  │   ├─ Upsert point in Qdrant
  │   ├─ Update MemoryUnit: embedding_id, updated_at
  │   ├─ Trigger promotion check
  │   └─ XACK processor_jobs slow_workers <message_id>
  │
  └─ On error: increment retry count → DLQ after 3 failures
```

**Consumer group:** multiple slow workers can run concurrently. Redis Streams `XREADGROUP` ensures each job is processed by exactly one worker.

### 10.3 Promotion Pipeline

MemoryOps has two live promotion paths.

**Access-based promotion (async per retrieval):**

```
1. Load MemoryUnit by id
2. Check Redis access counter (HGET memoryops:access:<id> count)
3. If: memory_type == Episodic
    AND importance_score >= promotion_threshold
    AND access_count >= access_count_trigger
    AND not deleted, not pinned
  → UPDATE memory_type = 'semantic'
4. Log promotion via tracing::info!
```

**Cluster-based promotion (nightly scheduler):**

```
1. Fetch all non-promoted episodic memories per workspace
2. Compute cosine similarity between pairs using Qdrant vectors
3. Group by similarity > dedup_threshold (default 0.92) into clusters
4. For qualifying clusters (avg importance >= promotion_threshold):
  a. Select highest-importance unit as canonical
  b. LLM summarize concatenated cluster content
  c. Write new Semantic MemoryUnit with merged source_events
  d. Soft-delete all cluster members
  e. Write Qdrant point for new semantic unit
  f. Emit AuditEntry: memory_promoted per affected id
```

`POST /v1/workspaces/:id/promote` manually triggers the cluster-based pass. The handler uses a Redis `SET NX EX 300` distributed lock per workspace to prevent concurrent runs.

**Configurable per workspace:** `promotion_threshold` (default: 0.85), `access_count_trigger` (default: 3), `dedup_threshold` (default: 0.92).

---

## 11. Retrieval Engine

### 11.1 Search Modes

The retrieval crate supports three search modes on `POST /v1/memory/search`:

| Mode | Strategy | Notes |
|------|----------|-------|
| `vector` | Qdrant ANN search | Requires `embedding_id`; falls back to empty if provider not configured |
| `keyword` | PostgreSQL `tsvector` FTS | `plainto_tsquery('english', query)` + GIN index |
| `hybrid` (default) | RRF fusion of vector + keyword | `score = Σ 1/(k + rank)` where k=60; normalised 0–1 |

### 11.2 RRF Fusion

```rust
pub const RRF_K: f32 = 60.0;

pub fn rrf_score(rank: u32) -> f32 {
    1.0 / (RRF_K + rank as f32)
}
```

Candidates from both legs are fused, scores normalised to `[0, 1]` by dividing by max raw score, then re-ranked. Vector result is preferred for tie-breaking.

### 11.3 Decay Scoring

```rust
pub fn decay_score(importance_score: f32, elapsed_secs: f64, half_life_secs: f64) -> f32 {
    // decay_score = importance_score × 0.5^(elapsed / half_life)
}
```

Default half-life: 30 days. Applied in bulk via `applydecayscoreswithhalflife` — skips pinned and importance-overridden units.

### 11.4 Access Tracking & Promotion

Every search result triggers:
1. `access::record_access(redis, memory_id)` — increments Redis HINCR counter (TTL 90 days)
2. `store::touch_last_accessed(db, id)` — async update of `last_accessed_at`
3. `promotion::check_and_promote(state, workspace_id, result_ids)` — async promotion eligibility check

### 11.5 List & CRUD

`GET /v1/memory` supports full filtering: `workspace_id`, `memory_type`, `pinned`, `min_importance`, sort by `importance_score | decay_score | updated_at | created_at`, direction `asc | desc`, and cursor pagination.

`PATCH /v1/memory/:id` supports: `content`, `pinned`, `importance_score` (sets `importance_overridden = true`), `tags`. Validates `importance_score ∈ [0.0, 1.0]`.

### 11.6 Score Breakdown

Full per-component score breakdown (`semantic_similarity`, `importance`, `recency`, `source_authority`, `memory_type_weight`) is tracked in `RetrievalTraceEntry`. `POST /v1/retrieve` is live as of M6 and returns packed memories with score breakdowns and trace data. Retrieval traces are persisted for 30 days in the `retrieval_traces` table and can be inspected through `GET /v1/retrieve/trace/:query_id`.

---

## 12. API Design

### 12.1 Conventions

- **Versioning:** All routes prefixed `/v1/`
- **Auth:** `X-API-Key` header required on all routes except `/v1/ingest/*` and `/health`
- **Content-Type:** `application/json` for all request/response bodies
- **Pagination:** cursor-based on list endpoints (`?after=<cursor>&limit=<n>`, default limit 20, max 100)
- **Errors:** unified error envelope (see §14)
- **Idempotency:** `POST` endpoints accept optional `Idempotency-Key` header
- **Timestamps:** all `DateTime` fields in ISO 8601 UTC (`2026-04-27T18:00:00Z`)
- **IDs:** all IDs are UUIDs v7 (time-ordered, sortable)

### 12.2 Error Envelope

```json
{
  "error": {
    "code": "memory_not_found",
    "message": "Memory unit with id 'abc...' not found",
    "request_id": "req_01hx..."
  }
}
```

### 12.3 Full API Surface

#### Ingestion (no auth, HMAC only)

| Method | Path | Status | Description |
|--------|------|--------|-------------|
| POST | `/v1/ingest/github` | ✅ Live | GitHub webhook receiver |
| POST | `/v1/ingest/slack` | ✅ Live | Slack Events API receiver |

#### Memory (live — M4 complete)

| Method | Path | Status | Description |
|--------|------|--------|-------------|
| GET | `/v1/memory` | ✅ Live | List (filter by type/pinned/importance/tag; sort; paginated) |
| GET | `/v1/memory/:id` | ✅ Live | Get with full entity, scope, score |
| PATCH | `/v1/memory/:id` | ✅ Live | Update: content, pin, tag, importance override |
| DELETE | `/v1/memory/:id` | ✅ Live | Soft delete |
| POST | `/v1/memory/search` | ✅ Live | Hybrid/vector/keyword search with RRF fusion |
| POST | `/v1/memory/:id/promote` | ✅ Live | Force episodic → semantic |
| POST | `/v1/memory/:id/restore` | ✅ Live | Restore soft-deleted memory |
| POST | `/v1/memory/bulk` | ✅ Live | Bulk pin / bulk delete |
| GET | `/v1/memory/:id/history` | ✅ Live | Version history |
| POST | `/v1/memory/merge` | ✅ Live | Merge two semantic memory units |

#### Retrieval

| Method | Path | Status | Description |
|--------|------|--------|-------------|
| POST | `/v1/retrieve` | ✅ Live | Core retrieval with token packing + trace |
| GET | `/v1/retrieve/trace/:query_id` | ✅ Live | Retrieval trace |

#### Workspaces

| Method | Path | Status | Description |
|--------|------|--------|-------------|
| POST | `/v1/workspaces` | ✅ Live | Create workspace |
| GET | `/v1/workspaces/:id` | ✅ Live | Get workspace |
| PATCH | `/v1/workspaces/:id/config` | ✅ Live | Update config |
| POST | `/v1/workspaces/:id/promote` | ✅ Live | Manual promotion pass (workspace-scoped lock) |
| POST | `/v1/workspaces/:id/integrations` | ✅ Live | Add integration |
| GET | `/v1/workspaces/:id/integrations` | ✅ Live | List with health |
| DELETE | `/v1/workspaces/:id/integrations/:source` | ✅ Live | Remove integration |

#### API Keys

| Method | Path | Status | Description |
|--------|------|--------|-------------|
| POST | `/v1/workspaces/:id/keys` | ✅ Live | Create key (plaintext returned once) |
| GET | `/v1/workspaces/:id/keys` | ✅ Live | List keys |
| DELETE | `/v1/workspaces/:id/keys/:key_id` | ✅ Live | Revoke key |

#### Audit & DLQ

| Method | Path | Status | Description |
|--------|------|--------|-------------|
| GET | `/v1/workspaces/:id/audit` | ✅ Live | Paginated audit log |
| GET | `/v1/workspaces/:id/dlq` | ✅ Live | List DLQ jobs |
| POST | `/v1/workspaces/:id/dlq/:job_id/retry` | ✅ Live | Retry DLQ job |
| DELETE | `/v1/workspaces/:id/dlq/:job_id` | ✅ Live | Discard DLQ job |

#### Export

| Method | Path | Status | Description |
|--------|------|--------|-------------|
| GET | `/v1/workspaces/:id/export` | ✅ Live | Stream JSONL export |

#### Health

| Method | Path | Description |
|--------|------|-------------|
| GET | `/health` | Liveness (no auth) |
| GET | `/health/ready` | Readiness: checks DB + Redis + Qdrant |

---

## 13. Database Conventions

### 13.1 General Rules

- All migrations in `migrations/` numbered sequentially: `0001_init.sql`
- Migrations are **additive only** — never drop columns, never rename in-place
- Deprecate columns by adding `_deprecated` suffix and a TODO migration
- All tables include `created_at TIMESTAMPTZ NOT NULL DEFAULT now()`
- All tables include `updated_at TIMESTAMPTZ NOT NULL DEFAULT now()` + trigger
- Soft deletes use `deleted_at TIMESTAMPTZ` (NULL = not deleted)
- All foreign keys have explicit `ON DELETE` behavior defined

### 13.2 Indexes

```sql
-- raw_events
CREATE INDEX idx_raw_events_workspace_occurred ON raw_events(workspace_id, occurred_at DESC);
CREATE UNIQUE INDEX idx_raw_events_idempotency ON raw_events(idempotency_key);

-- memory_units
CREATE INDEX idx_memory_units_workspace_type ON memory_units(workspace_id, memory_type);
CREATE INDEX idx_memory_units_decay ON memory_units(workspace_id, decay_score) WHERE deleted_at IS NULL;
CREATE INDEX idx_memory_units_scope ON memory_units(workspace_id, agent_id, user_id, repo);
CREATE INDEX idx_memory_units_tags ON memory_units USING gin(tags);
-- M4 additions:
CREATE INDEX idx_memory_units_fts ON memory_units USING GIN(to_tsvector('english', content));
CREATE INDEX idx_memory_units_workspace_type_score ON memory_units(workspace_id, memory_type, importance_score DESC) WHERE deleted_at IS NULL;
CREATE INDEX idx_memory_units_workspace_pinned ON memory_units(workspace_id, pinned, updated_at DESC) WHERE deleted_at IS NULL AND pinned = true;

-- audit_log
CREATE INDEX idx_audit_log_workspace_time ON audit_log(workspace_id, occurred_at DESC);
```

### 13.3 Updated_at Trigger

```sql
CREATE OR REPLACE FUNCTION set_updated_at()
RETURNS TRIGGER AS $$
BEGIN
  NEW.updated_at = now();
  RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trg_memory_units_updated_at
  BEFORE UPDATE ON memory_units
  FOR EACH ROW EXECUTE FUNCTION set_updated_at();
```

### 13.4 Query Patterns

- All queries receive `workspace_id` as a bind param — never interpolated
- Use `sqlx::query_as!` macro for compile-time SQL checking
- Use `RETURNING *` on inserts to avoid round-trips
- Never `SELECT *` in application code — always explicit column lists
- Connection pool: `max_connections = 20` (configurable), `min_connections = 2`

---

## 14. Error Handling

### 14.1 Unified AppError

```rust
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("not found: {resource}")]
    NotFound { resource: String },

    #[error("unauthorized")]
    Unauthorized,

    #[error("forbidden: insufficient scope")]
    Forbidden,

    #[error("validation error: {0}")]
    Validation(String),

    #[error("conflict: {0}")]
    Conflict(String),

    #[error("rate limited")]
    RateLimited { retry_after_secs: u64 },

    #[error("provider error: {0}")]
    Provider(#[from] ProviderError),

    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),

    #[error("internal error")]
    Internal(#[from] anyhow::Error),
}
```

### 14.2 axum IntoResponse

```rust
impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, code) = match &self {
            AppError::NotFound { .. }   => (StatusCode::NOT_FOUND, "not_found"),
            AppError::Unauthorized      => (StatusCode::UNAUTHORIZED, "unauthorized"),
            AppError::Forbidden         => (StatusCode::FORBIDDEN, "forbidden"),
            AppError::Validation(_)     => (StatusCode::BAD_REQUEST, "validation_error"),
            AppError::Conflict(_)       => (StatusCode::CONFLICT, "conflict"),
            AppError::RateLimited{..}   => (StatusCode::TOO_MANY_REQUESTS, "rate_limited"),
            AppError::Provider(_)       => (StatusCode::BAD_GATEWAY, "provider_error"),
            AppError::Database(_)
            | AppError::Internal(_)     => (StatusCode::INTERNAL_SERVER_ERROR, "internal_error"),
        };

        if status == StatusCode::INTERNAL_SERVER_ERROR {
            tracing::error!(error = ?self, "internal error");
        }

        Json(json!({
            "error": {
                "code": code,
                "message": self.to_string(),
            }
        })).into_response()
    }
}
```

### 14.3 Rules

- **Never** `unwrap()` or `expect()` in non-test code — use `?` propagation
- **Never** expose internal error details (DB messages, stack traces) to clients
- All errors logged with `tracing::error!` at the handler boundary, not deep in call stack
- `anyhow::Context` used to add context when converting errors across crate boundaries
- Panics are treated as bugs — `tokio::spawn` tasks catch panics and emit error spans

---

## 15. Testing Strategy

### 15.1 Pyramid

```
        ┌──────────────┐
        │   E2E Tests  │  (few, slow)  docker-compose.test.yml
        ├──────────────┤
        │ Integration  │  (moderate)   real DB + Redis + Qdrant
        ├──────────────┤
        │  Unit Tests  │  (many, fast) pure logic, no I/O
        └──────────────┘
```

### 15.2 Unit Tests

- Location: `#[cfg(test)]` module at bottom of each source file
- Coverage targets:
  - RRF fusion: rank ordering, score normalisation, tie-breaking
  - Decay formula: half-life boundary conditions
  - Keyword search builder: filter pushdown SQL correctness
  - Promotion eligibility: all flag combinations
  - Webhook validator: valid HMAC, invalid HMAC, stale timestamp
  - MemoryScope specificity ordering

### 15.3 Integration Tests

- Location: `tests/` directory per crate
- Use `sqlx::test` macro for DB tests — each test gets a fresh schema
- Use `wiremock` for mocking LLM and embedding provider HTTP calls
- Live-service tests tagged `#[ignore]` — require `docker-compose.test.yml`

### 15.4 E2E Tests

- Spin up full stack via `docker-compose.test.yml`
- Cover: ingest → process → search → verify memory appears in results
- Cover: promotion trigger via repeated access
- Run on every PR, block merge on failure

### 15.5 Property-Based Tests

- Use `proptest` for token packing algorithm — verify invariants:
  - Packed tokens never exceed `token_budget`
  - No two included memories have similarity > `dedup_threshold`
  - All excluded memories appear in trace

### 15.6 Coverage

- Minimum 80% line coverage enforced in CI via `cargo-llvm-cov`
- Coverage report uploaded as CI artifact

---

## 16. Observability

### 16.1 Structured Logging

- All logs via `tracing` crate — no `println!` or `eprintln!` in application code
- Log format: JSON in production, pretty-printed in development
- Required fields on every request span: `request_id`, `workspace_id`, `method`, `path`, `status_code`, `latency_ms`

### 16.2 Metrics (OpenTelemetry)

| Metric | Type | Labels |
|--------|------|--------|
| `memoryops_ingest_events_total` | Counter | source, event_type, status |
| `memoryops_processor_job_duration_ms` | Histogram | path (fast/slow) |
| `memoryops_retrieval_duration_ms` | Histogram | mode (vector/keyword/hybrid) |
| `memoryops_memory_units_total` | Gauge | workspace_id, memory_type |
| `memoryops_decay_updated_total` | Counter | workspace_id |
| `memoryops_dlq_jobs_total` | Gauge | status (pending/failed) |
| `memoryops_provider_latency_ms` | Histogram | provider, operation |

### 16.3 Health Checks

```rust
// GET /health/ready
// Returns 200 only if all dependencies are healthy
pub async fn readiness() -> impl IntoResponse {
    let (database, redis, qdrant) = tokio::join!(
        check_database(),
        check_redis(),
        check_qdrant()
    );
    let ready = database.is_ready() && redis.is_ready() && qdrant.is_ready();
    let status = if ready { StatusCode::OK } else { StatusCode::SERVICE_UNAVAILABLE };
    (status, Json(json!({ "status": if ready { "ok" } else { "unavailable" }, "checks": { ... } })))
}
```

---

## 17. CI/CD Pipeline

### 17.1 CI (`.github/workflows/ci.yml`)

Triggered on: every push, every PR.

```yaml
jobs:
  check:
    - cargo fmt --check
    - cargo clippy -- -D warnings
    - cargo audit

  test:
    - cargo test --workspace
    - cargo llvm-cov --lcov --output-path lcov.info
    - Upload coverage artifact

  integration:
    services: [postgres, redis, qdrant]
    - cargo test --workspace -- --ignored
```

### 17.2 Branch Strategy

- `main` — always deployable; protected, requires PR + CI pass
- `feature/*` — feature branches; squash-merged to main
- `fix/*` — bug fixes
- No long-lived branches

### 17.3 Commit Convention

Follows [Conventional Commits](https://www.conventionalcommits.org/):

```
feat(retrieval): add hybrid RRF search
fix(processor): correct decay half-life formula
chore(deps): update axum to 0.8
docs(spec): update milestones — UI moved to M5
```

---

## 18. Multi-Tenancy

- Every table includes `workspace_id UUID NOT NULL`
- `workspace_id` is always a bind param — enforced at `sqlx` level, not application layer
- No cross-workspace queries possible — no global list endpoints without workspace scope
- `MemoryScope` provides sub-workspace granularity (agent / user / repo)
- Row-level isolation verified by integration tests that assert workspace A cannot access workspace B data

---

## 19. Decay & Pruning

```
decay_score(t) = importance_score × 0.5^(elapsed_secs / half_life_secs)
```

| Parameter | Default | Configurable |
|-----------|---------|-------------|
| `half_life_days` | 30 | Per workspace (future) |
| Pruning threshold | 0.10 | Per workspace (future) |
| Pinned memories | Skipped entirely | N/A |
| `importance_overridden` | Skipped | N/A |

**Scheduler behavior:**
- `run_decay_pass(state, workspace_id)` exported from retrieval crate
- Processes all non-pinned, non-overridden units in a single UPDATE
- Runs daily at 02:00 UTC (scheduler — M6+)
- Hard delete: separate job runs 30 days after `deleted_at`

---

## 20. Rate Limiting

Implemented via Redis sliding window counter.

```
Key pattern: rate:{workspace_id}:{endpoint_group}:{window_start_unix}
Algorithm:   INCR + EXPIREAT per window
```

All limits configurable per workspace. Exceeding returns `429 Too Many Requests`.

### 20.1 Rate Limit Groups

| Group | Routes | Limit |
|-------|--------|-------|
| `ingest` | `/v1/ingest/*` | 300 RPM per workspace |
| `memory` | `/v1/memory/*` | 120 RPM per workspace |
| `api` | All other `/v1/*` | 120 RPM per workspace |

Window: 60s sliding. Excess → `429 Too Many Requests` with `Retry-After`.

---

## 21. Frontend Architecture

### 21.1 Stack

| Concern | Choice | Reason |
|---------|--------|--------|
| Framework | React 19 + TypeScript | Stable, wide ecosystem |
| Build | Vite | Fast HMR, ESM-native |
| State | Zustand | Minimal, no boilerplate |
| Data fetching | TanStack Query | Cache + stale-while-revalidate |
| Styling | Tailwind CSS v4 | Utility-first, consistent |
| Components | shadcn/ui | Accessible, composable |
| Tables | TanStack Table | Powerful, headless |
| Charts | Recharts | Lightweight, declarative |
| Testing | Vitest + React Testing Library | Fast, co-located |
| E2E | Playwright | Cross-browser |

### 21.2 Views (M5 scope — targeting live endpoints)

| Route | Component | Live Endpoints Used |
|-------|-----------|-------------------|
| `/` | Dashboard | `GET /health/ready`, `GET /v1/memory` (counts) |
| `/memory` | MemoryExplorer | `GET /v1/memory`, `POST /v1/memory/search`, `PATCH /v1/memory/:id` |
| `/memory/:id` | MemoryDetail | `GET /v1/memory/:id`, `PATCH /v1/memory/:id` |
| `/ingest` | WebhookTester | `POST /v1/ingest/github` (dev tool) |
| `/settings` | WorkspaceSettings | config display only (M6 write) |

Views for `/retrieve/trace`, `/lifecycle`, `/audit`, `/integrations` are stubbed with empty states in M5 and wired to real endpoints in M6+.

### 21.3 State Management

```typescript
// Global store — only for cross-cutting concerns
interface AppStore {
  workspaceId: string;
  apiKey: string;  // stored in memory only, never localStorage
  setWorkspace: (id: string, key: string) => void;
}

// Server state — TanStack Query handles caching
const { data: memories } = useQuery({
  queryKey: ['memories', workspaceId, filters],
  queryFn: () => MemoryApi.list({ workspaceId, ...filters }),
  staleTime: 30_000,
});
```

**Security:** API key stored in memory (Zustand store) only. Never written to `localStorage`, `sessionStorage`, or cookies. Cleared on tab close.

### 21.4 Component Rules

- No business logic in components — logic lives in custom hooks
- All data fetching in hooks, not components
- Every interactive component has a `data-testid` attribute for testing
- No inline styles — Tailwind only
- Accessibility: all interactive elements have ARIA labels; keyboard-navigable

---

## 22. Audit Log

```rust
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct AuditEntry {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub actor: String,           // API key name or "system" or "scheduler"
    pub action: AuditAction,
    pub target_id: Uuid,
    pub target_type: String,
    pub diff: Option<serde_json::Value>,
    pub occurred_at: DateTime<Utc>,
}
```

Audit writes are fire-and-forget (`tokio::spawn`) — never block the primary request path.

---

## 23. Integration Health & DLQ

### 23.1 Health Tracking

```rust
pub struct IntegrationHealth {
    pub workspace_id: Uuid,
    pub source: Source,
    pub last_event_at: Option<DateTime<Utc>>,
    pub events_24h: i64,
    pub errors_24h: i64,
    pub status: IntegrationStatus,
}
```

### 23.2 Dead Letter Queue

- Backend: Redis list `dlq:{workspace_id}`
- Auto-retry: 3 attempts with exponential backoff (1s, 4s, 16s)
- After 3 failures: written to DLQ
- DLQ entries expire after 7 days

---

## 24. MCP Server

M11 introduces a dedicated `crates/mcp/` server crate that exposes MemoryOps retrieval and storage workflows through the Model Context Protocol 2025-06-18 specification. The crate remains a scaffold during M10 and is not part of the runtime path until M11.

### 24.1 Tools

| Tool | Purpose | Backend Contract |
|------|---------|------------------|
| `memory_retrieve` | Return token-packed memory context with trace data | Wraps `POST /v1/retrieve` |
| `memory_search` | Search memory units without token packing | Wraps `POST /v1/memory/search` |
| `memory_store` | Store one memory unit directly for agent-authored memories | Writes through the ingestion/memory path with workspace ownership |

Tool inputs must include enough scope information to resolve the target workspace and optional agent/user/repo scope. Tool outputs mirror the REST response schemas where practical so clients can share DTO handling.

### 24.2 Transports

| Transport | Status | Notes |
|-----------|--------|-------|
| stdio | Planned for local agent runtimes and editor-launched processes | MCP request/response over stdin/stdout |
| HTTP SSE | Planned for daemonized deployments | HTTP endpoint with server-sent event stream per MCP 2025-06-18 |

The docker-compose MCP endpoint is planned for M11 and should run separately from the REST API process while sharing the same Postgres, Redis, Qdrant, provider config, and workspace auth model.

### 24.3 Auth

MCP clients authenticate with a workspace API key during the MCP `initialize` handshake. The client passes the key as a Bearer token, and the MCP server validates it against the same workspace API key store used by REST requests. The resolved workspace becomes the default workspace context for all subsequent MCP tool calls on that session. Tools must reject cross-workspace IDs that do not match the initialized workspace.

---

## 25. Export & Backup

- `GET /v1/workspaces/:id/export` — streams JSONL, one memory unit per line
- Uses chunked transfer encoding — no in-memory buffer for large workspaces
- Excludes: `payload` from raw events (may contain PII), embeddings (raw vectors)

---

## 26. Code Quality Standards

### 25.1 Rust

- **Format:** `rustfmt` enforced in CI
- **Lint:** `clippy` with `-D warnings`; all warnings treated as errors in CI
- **No `unwrap()`/`expect()`** outside tests
- **No `clone()`** on large structs in hot paths — prefer `Arc<T>`
- **Dead code:** `#[allow(dead_code)]` is forbidden
- **Unsafe:** forbidden unless in a dedicated file with safety comment
- **MSRV:** Rust 1.88.0 stable, pinned in `rust-toolchain.toml`

### 25.2 TypeScript

- **Strict mode:** `"strict": true` in `tsconfig.json`
- **Format:** Prettier enforced in CI
- **Lint:** ESLint with `@typescript-eslint` + `react-hooks` rules
- **No `any`:** Use `unknown` + type guards

### 25.3 SQL

- All queries use parameterized binding — no string interpolation ever
- Explain-analyze run on all new queries before merge
- Migrations tested in integration suite before merge

---

## 27. Non-Goals (v0.1)

- Generic "second brain" or consumer product
- Full agent runtime or framework
- Vector database replacement
- Real-time streaming ingestion
- Custom embedding model fine-tuning
- Push-mode context injection (agent SDK — v0.3+)
- SSO / OAuth login (v0.3+ SaaS phase)
- Multi-region deployment
- Import from export (v0.3)

---

## 28. Risks & Mitigations

| Risk | Likelihood | Impact | Mitigation |
|------|-----------|--------|-----------|
| LLM latency blocks throughput | Medium | High | Async-only slow path; local Ollama default |
| Large context windows reduce retrieval need | Low | Medium | Moat is lifecycle + governance, not retrieval size |
| Retrieval commoditized by frameworks | Medium | Medium | Differentiator is ingestion + control UI + trace |
| GitHub/Slack API changes break parsers | Low | High | Versioned event types; parser tests against fixtures |
| Qdrant unavailability degrades retrieval | Low | High | Fallback to keyword-only if Qdrant unreachable |
| Webhook replay attacks | Low | Medium | Idempotency key + timestamp validation (5min window) |
| Postgres migration failure in prod | Low | Critical | Always run migrations in staging first; rollback plan per migration |

---

## 29. Milestones

| # | Deliverable | Key Acceptance Criteria | Status |
|---|-------------|------------------------|--------|
| M1 | Rust workspace + docker-compose + migrations scaffold | `cargo build` clean; `docker compose up` starts all infra | ✅ Complete |
| M2 | GitHub webhook ingestion + RawEvent writes | Webhook delivers → `raw_events` row exists; idempotency works | ✅ Complete |
| M3 | Fast path processor + episodic MemoryUnit | Ingest PR → MemoryUnit with entities + importance created | ✅ Complete |
| M4 | Retrieval crate — search, list, get, update, decay, promotion | `POST /v1/memory/search` returns ranked results; hybrid RRF works; decay pass runs; `cargo test` passes | ✅ Complete |
| M5 | React Memory Control Center (against live API) | Explorer, detail, search views functional; webhook tester fires real ingestion | ✅ Complete |
| M6 | Full REST API + auth + rate limiting + audit + soft delete | All endpoints passing integration tests; 401/429 enforced; `POST /v1/retrieve` live | ✅ Complete |
| M7 | Slow path worker + embeddings + Qdrant write | MemoryUnit gets `embedding_id`; Qdrant point queryable; vector leg of hybrid search active | ✅ Complete |
| M8 | Promotion pipeline (batch clustering) | Episodic cluster → Semantic MemoryUnit after threshold | ✅ Complete |
| M9 | Slack ingestion | Slack message → MemoryUnit via same pipeline | ✅ Complete |
| M10 | Linear + Jira ingestion | Linear/Jira webhooks validate signatures, normalize supported events to RawEvent, enqueue processor jobs, and produce MemoryUnits with source-specific scoring/entities | 🔴 Planned |
| M11 | MCP server | `crates/mcp/` exposes `memory_retrieve`, `memory_search`, and `memory_store` over stdio and HTTP SSE with workspace API key auth | 🔴 Planned |
| M12 | Lifecycle configuration | Workspace config controls decay half-life and pruning threshold per workspace | 🔴 Planned |
