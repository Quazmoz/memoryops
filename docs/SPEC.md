# MemoryOps — Technical Specification

**Version:** 0.3.0  
**Status:** Active  
**Last Updated:** 2026-04-12

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
24. [Export & Backup](#24-export--backup)
25. [Code Quality Standards](#25-code-quality-standards)
26. [Non-Goals](#26-non-goals)
27. [Risks & Mitigations](#27-risks--mitigations)
28. [Milestones](#28-milestones)

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
  │                 # skipped for /webhooks/* and /health
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
│  │ POST /webhooks/  │───▶│  Fast Worker  │  Slow Worker  │   │
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
│  │  Qdrant (semantic) + Tantivy (BM25)  │                     │
│  │  Scorer → Token Packer → Trace       │                     │
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
│   │   │   ├── router.rs       # axum routes for /webhooks/*
│   │   │   ├── validator.rs    # WebhookValidator trait + impls
│   │   │   ├── github/
│   │   │   │   ├── mod.rs
│   │   │   │   ├── handler.rs
│   │   │   │   └── parser.rs   # GitHub payload → RawEvent
│   │   │   └── slack/
│   │   │       ├── mod.rs
│   │   │       ├── handler.rs
│   │   │       └── parser.rs
│   ├── processor/
│   │   ├── src/
│   │   │   ├── lib.rs
│   │   │   ├── fast/
│   │   │   │   ├── mod.rs
│   │   │   │   ├── extractor.rs    # entity extraction
│   │   │   │   ├── tagger.rs       # tag assignment
│   │   │   │   └── scorer.rs       # importance scoring rules
│   │   │   ├── slow/
│   │   │   │   ├── mod.rs
│   │   │   │   ├── worker.rs       # Redis consumer loop
│   │   │   │   ├── summarizer.rs   # LlmProvider::summarize wrapper
│   │   │   │   └── embedder.rs     # EmbeddingProvider + Qdrant write
│   │   │   └── promotion/
│   │   │       ├── mod.rs
│   │   │       ├── clusterer.rs
│   │   │       └── promoter.rs
│   ├── retrieval/
│   │   ├── src/
│   │   │   ├── lib.rs
│   │   │   ├── search.rs       # hybrid Qdrant + Tantivy
│   │   │   ├── scorer.rs       # scoring formula
│   │   │   ├── packer.rs       # token packing algorithm
│   │   │   └── trace.rs        # RetrievalTrace builder + persistence
│   └── api/
│       ├── src/
│       │   ├── main.rs
│       │   ├── router.rs
│       │   ├── middleware/
│       │   │   ├── auth.rs
│       │   │   └── rate_limit.rs
│       │   └── handlers/
│       │       ├── memory.rs
│       │       ├── retrieve.rs
│       │       ├── workspaces.rs
│       │       ├── keys.rs
│       │       └── audit.rs
├── frontend/                   # React + TypeScript
│   ├── src/
│   │   ├── components/
│   │   ├── pages/
│   │   ├── hooks/
│   │   ├── api/                # typed API client (generated from OpenAPI)
│   │   └── stores/             # Zustand state
│   └── package.json
├── migrations/                 # sqlx numbered migrations
│   ├── 0001_init.sql
│   ├── 0002_memory_units.sql
│   └── ...
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

All shared state is injected via `axum::Extension` — never global statics.

```rust
#[derive(Clone)]
pub struct AppState {
    pub db: PgPool,
    pub redis: ConnectionManager,
    pub qdrant: QdrantClient,
    pub embedding_provider: Arc<dyn EmbeddingProvider>,
    pub llm_provider: Arc<dyn LlmProvider>,
    pub config: Arc<AppConfig>,
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
|-------|------------------|
| `pull_request` | opened, closed, merged, review_requested |
| `pull_request_review` | submitted (approved, changes_requested, commented) |
| `push` | all refs; extract commits, author, branch |
| `issue_comment` | created, edited |
| `issues` | opened, closed, labeled |

### 9.3 Ingestion Flow

```
POST /webhooks/github
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
|-------|---------------|
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

Runs as a scheduled job (every 15 minutes, configurable).

```
1. Fetch Episodic MemoryUnits: age > clustering_window AND not promoted
2. Cluster by (entity_overlap + topic_similarity + time_window)
3. For each cluster:
   a. Compute cluster importance = mean(importance_scores)
   b. If cluster_importance >= promotion_threshold (default: 0.65):
      i.  Call LlmProvider::summarize(joined_cluster_content)
      ii. Call EmbeddingProvider::embed(summary)
      iii. INSERT Semantic MemoryUnit
      iv. Mark source Episodic units as promoted
      v.  Write audit entry
```

**Configurable per workspace:** `clustering_window` (default: 24h), `promotion_threshold` (default: 0.65), `promotion_cadence` (default: 15min).

---

## 11. Retrieval Engine

### 11.1 Request

```rust
#[derive(Debug, Deserialize, Validate)]
pub struct RetrievalRequest {
    pub scope: Option<MemoryScope>,
    #[validate(length(min = 1, max = 2000))]
    pub query: String,
    #[validate(range(min = 256, max = 32768))]
    pub token_budget: usize,
    pub filters: Option<RetrievalFilters>,
}

#[derive(Debug, Deserialize)]
pub struct RetrievalFilters {
    pub sources: Option<Vec<Source>>,
    pub memory_types: Option<Vec<MemoryType>>,
    pub entities: Option<Vec<String>>,
    pub since: Option<DateTime<Utc>>,
    pub tags: Option<Vec<String>>,
    pub min_importance: Option<f32>,
}
```

`workspace_id` is injected from the authenticated API key — never trusted from the request body.

### 11.2 Scoring Formula

```
final_score =
    (semantic_similarity  × w1)   # default 0.35
  + (importance_score     × w2)   # default 0.25
  + (recency_score        × w3)   # default 0.20
  + (source_authority     × w4)   # default 0.10
  + (memory_type_weight   × w5)   # default 0.10
```

Weights `w1–w5` must sum to `1.0`. Validated on workspace config update.

**Component formulas:**
- `recency_score = 1.0 / (1.0 + days_since_event)`
- `source_authority` = workspace-configured float per source (GitHub: `0.9`, Slack: `0.6`)
- `memory_type_weight` = Semantic: `1.0`, Episodic: `0.5`

```rust
pub struct ScoringWeights {
    pub semantic_similarity: f32,
    pub importance: f32,
    pub recency: f32,
    pub source_authority: f32,
    pub memory_type: f32,
}

impl ScoringWeights {
    pub fn validate(&self) -> Result<(), ConfigError> {
        let sum = self.semantic_similarity + self.importance
            + self.recency + self.source_authority + self.memory_type;
        if (sum - 1.0).abs() > 1e-4 {
            return Err(ConfigError::WeightsMustSumToOne);
        }
        Ok(())
    }
}
```

### 11.3 Token Packing Algorithm

```rust
pub fn pack(
    candidates: Vec<ScoredMemory>,
    token_budget: usize,
    dedup_threshold: f32,  // default 0.92
) -> (Vec<ScoredMemory>, Vec<RetrievalTraceEntry>) {
    // 1. Sort by final_score desc
    // 2. Greedy inclusion under token_budget
    // 3. Before including: check cosine similarity against
    //    all already-included embeddings
    //    → skip if similarity > dedup_threshold
    // 4. Track all candidates in trace (included + excluded)
    // 5. Return (packed, trace)
}
```

Token counting uses `tiktoken-rs` (cl100k_base tokenizer — compatible with GPT-4 and most modern models). Count is computed once per memory at embedding time and cached in `token_count` column.

### 11.4 Response

```rust
#[derive(Debug, Serialize)]
pub struct RetrievalResult {
    pub query_id: Uuid,
    pub memories: Vec<ScoredMemory>,
    pub total_tokens: usize,
    pub trace: Vec<RetrievalTraceEntry>,
    pub retrieved_at: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
pub struct ScoreBreakdown {
    pub semantic_similarity: f32,
    pub importance: f32,
    pub recency: f32,
    pub source_authority: f32,
    pub memory_type: f32,
    pub final_score: f32,
}

#[derive(Debug, Serialize)]
pub struct RetrievalTraceEntry {
    pub memory_id: Uuid,
    pub breakdown: ScoreBreakdown,
    pub token_count: usize,
    pub included: bool,
    pub exclusion_reason: Option<ExclusionReason>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ExclusionReason {
    TokenBudget,
    Dedup,
    Filtered,
    BelowMinImportance,
}
```

Traces persisted to Postgres for 30 days. Queryable by `query_id`.

---

## 12. API Design

### 12.1 Conventions

- **Versioning:** All routes prefixed `/v1/`
- **Auth:** `X-API-Key` header required on all routes except `/webhooks/*` and `/health`
- **Content-Type:** `application/json` for all request/response bodies
- **Pagination:** cursor-based on all list endpoints (`?after=<cursor>&limit=<n>`, default limit 20, max 100)
- **Errors:** unified error envelope (see §14)
- **Idempotency:** `POST` endpoints accept optional `Idempotency-Key` header
- **Timestamps:** all `DateTime` fields in ISO 8601 UTC (`2026-04-12T23:00:00Z`)
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

#### Webhooks (no auth, HMAC only)

| Method | Path | Description |
|--------|------|-------------|
| POST | `/v1/webhooks/github` | GitHub webhook receiver |
| POST | `/v1/webhooks/slack` | Slack Events API receiver |

#### Memory

| Method | Path | Description |
|--------|------|-------------|
| GET | `/v1/memory` | List (cursor paginated; filter by type/source/tag/scope/entity) |
| GET | `/v1/memory/:id` | Get with full entity, lineage, score |
| PATCH | `/v1/memory/:id` | Update: pin, tag, content, importance override |
| DELETE | `/v1/memory/:id` | Soft delete |
| POST | `/v1/memory/:id/promote` | Force episodic → semantic |
| POST | `/v1/memory/:id/restore` | Restore soft-deleted memory |
| POST | `/v1/memory/bulk` | Bulk pin / bulk delete (filter or ID list) |
| GET | `/v1/memory/:id/history` | Version history |
| POST | `/v1/memory/merge` | Merge two semantic memory units |

#### Retrieval

| Method | Path | Description |
|--------|------|-------------|
| POST | `/v1/retrieve` | Core retrieval |
| GET | `/v1/retrieve/trace/:query_id` | Retrieval trace |

#### Workspaces

| Method | Path | Description |
|--------|------|-------------|
| POST | `/v1/workspaces` | Create workspace |
| GET | `/v1/workspaces/:id` | Get workspace |
| PATCH | `/v1/workspaces/:id/config` | Update config (weights, thresholds, providers) |
| POST | `/v1/workspaces/:id/integrations` | Add integration |
| GET | `/v1/workspaces/:id/integrations` | List with health |
| DELETE | `/v1/workspaces/:id/integrations/:source` | Remove integration |

#### API Keys

| Method | Path | Description |
|--------|------|-------------|
| POST | `/v1/workspaces/:id/keys` | Create key (plaintext returned once) |
| GET | `/v1/workspaces/:id/keys` | List keys |
| DELETE | `/v1/workspaces/:id/keys/:key_id` | Revoke key |

#### Audit & DLQ

| Method | Path | Description |
|--------|------|-------------|
| GET | `/v1/workspaces/:id/audit` | Paginated audit log |
| GET | `/v1/workspaces/:id/dlq` | List DLQ jobs |
| POST | `/v1/workspaces/:id/dlq/:job_id/retry` | Retry DLQ job |
| DELETE | `/v1/workspaces/:id/dlq/:job_id` | Discard DLQ job |

#### Export

| Method | Path | Description |
|--------|------|-------------|
| GET | `/v1/workspaces/:id/export` | Stream JSONL export |

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

        // Internal errors: log full detail, return sanitized message to client
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
  - Scoring formula: all weight combinations
  - Token packing: boundary conditions, dedup logic
  - Entity extractor: regex patterns against fixture payloads
  - Importance scorer: all rule table entries
  - Webhook validator: valid HMAC, invalid HMAC, stale timestamp
  - MemoryScope specificity ordering

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scoring_weights_must_sum_to_one() {
        let weights = ScoringWeights {
            semantic_similarity: 0.40,
            importance: 0.25,
            recency: 0.20,
            source_authority: 0.10,
            memory_type: 0.10,  // sum = 1.05 → should fail
        };
        assert!(weights.validate().is_err());
    }
}
```

### 15.3 Integration Tests

- Location: `tests/` directory per crate
- Use `sqlx::test` macro for DB tests — each test gets a fresh schema
- Use `wiremock` for mocking LLM and embedding provider HTTP calls
- Use `testcontainers` for Qdrant and Redis in CI

```rust
#[sqlx::test]
async fn test_ingest_github_pr_event(pool: PgPool) {
    let state = test_state(pool).await;
    let payload = fixture("github_pr_opened.json");
    let response = post_webhook("/v1/webhooks/github", payload, &state).await;
    assert_eq!(response.status(), 202);
    let event = fetch_latest_raw_event(&state.db).await;
    assert_eq!(event.event_type, EventType::PullRequest);
}
```

### 15.4 E2E Tests

- Spin up full stack via `docker-compose.test.yml`
- Cover: ingest → process → retrieve → verify context contains expected memory
- Cover: agent failure scenario (no memory) vs. correct behavior (with memory)
- Run on every PR, block merge on failure

### 15.5 Property-Based Tests

- Use `proptest` for token packing algorithm — verify invariants:
  - Packed tokens never exceed `token_budget`
  - No two included memories have similarity > `dedup_threshold`
  - All excluded memories appear in trace

### 15.6 Coverage

- Minimum 80% line coverage enforced in CI via `cargo-llvm-cov`
- Coverage report uploaded as CI artifact
- Coverage badge in README

---

## 16. Observability

### 16.1 Structured Logging

- All logs via `tracing` crate — no `println!` or `eprintln!` in application code
- Log format: JSON in production, pretty-printed in development
- Required fields on every request span:
  - `request_id` (UUID v7)
  - `workspace_id`
  - `method`, `path`, `status_code`, `latency_ms`

```rust
// Good
tracing::info!(
    workspace_id = %workspace_id,
    memory_id = %id,
    "memory promoted to semantic"
);

// Bad
println!("promoted memory {}", id);
```

### 16.2 Metrics (OpenTelemetry)

| Metric | Type | Labels |
|--------|------|--------|
| `memoryops_ingest_events_total` | Counter | source, event_type, status |
| `memoryops_processor_job_duration_ms` | Histogram | path (fast/slow) |
| `memoryops_retrieval_duration_ms` | Histogram | |
| `memoryops_retrieval_tokens_packed` | Histogram | |
| `memoryops_memory_units_total` | Gauge | workspace_id, memory_type |
| `memoryops_decay_pruned_total` | Counter | memory_type |
| `memoryops_dlq_jobs_total` | Gauge | status (pending/failed) |
| `memoryops_provider_latency_ms` | Histogram | provider, operation |

Exported via OTEL exporter (configurable: Prometheus scrape or OTLP push).

### 16.3 Tracing

- Distributed traces via OTEL — spans across ingestion → processing → retrieval
- `trace_id` propagated in `X-Trace-ID` response header
- Slow spans (> 500ms) logged at WARN level automatically

### 16.4 Health Checks

```rust
// GET /health/ready
// Returns 200 only if all dependencies are healthy
pub async fn readiness(State(state): State<AppState>) -> impl IntoResponse {
    let db_ok = state.db.acquire().await.is_ok();
    let redis_ok = ping_redis(&state.redis).await.is_ok();
    let qdrant_ok = state.qdrant.health_check().await.is_ok();

    if db_ok && redis_ok && qdrant_ok {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    }
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
    - cargo test --all
    - cargo llvm-cov --lcov --output-path lcov.info
    - Upload coverage artifact

  integration:
    services: [postgres, redis, qdrant]
    - cargo test --test '*' --all

  e2e:
    - docker compose -f docker-compose.test.yml up -d
    - cargo test --test e2e
    - docker compose down

  openapi:
    - Validate openapi.yaml against routes (spectral lint)
    - Diff openapi.yaml against previous — fail on breaking changes
```

### 17.2 Release (`.github/workflows/release.yml`)

Triggered on: tag push `v*`.

```yaml
jobs:
  build:
    - cargo build --release --target x86_64-unknown-linux-musl
    - Build Docker image (multi-stage)
    - Push to registry

  migrate:
    - Run sqlx migrate run against target environment

  deploy:
    - Rolling deploy (zero-downtime)
```

### 17.3 Branch Strategy

- `main` — always deployable; protected, requires PR + CI pass
- `feature/*` — feature branches; squash-merged to main
- `fix/*` — bug fixes
- No long-lived branches

### 17.4 Commit Convention

Follows [Conventional Commits](https://www.conventionalcommits.org/):

```
feat(ingestion): add GitHub push event handler
fix(retrieval): correct token count off-by-one in packer
chore(deps): update axum to 0.8
docs(spec): add testing strategy section
```

Enforced via `commitlint` in CI.

---

## 18. Multi-Tenancy

- Every table includes `workspace_id UUID NOT NULL`
- `workspace_id` is always a bind param — enforced at `sqlx` level, not application layer
- No cross-workspace queries possible — no global list endpoints without workspace scope
- `MemoryScope` provides sub-workspace granularity (agent / user / repo)
- Workspace config table controls: scoring weights, thresholds, provider selection, rate limits, scope dimensions
- Row-level isolation verified by integration tests that assert workspace A cannot access workspace B data

---

## 19. Decay & Pruning

```
decay_score(t) = initial × (decay_rate ^ days_since_last_access)
```

| Parameter | Default (Semantic) | Default (Episodic) | Configurable |
|-----------|-------------------|-------------------|-------------|
| `decay_rate` | 0.98 | 0.95 | Per workspace |
| Pruning threshold | 0.10 | 0.10 | Per workspace |
| Recovery window | 30 days | 30 days | Fixed |

**Scheduler behavior:**
- Runs daily at 02:00 UTC (configurable)
- Processes in batches of 1000 to avoid table locks
- Pinned memories: skipped entirely
- On prune: set `deleted_at = now()` (soft delete), write audit entry
- Hard delete: separate job runs 30 days after `deleted_at`

---

## 20. Rate Limiting

Implemented via Redis sliding window counter.

```
Key pattern: rate:{workspace_id}:{endpoint_group}:{window_start_unix}
Algorithm:   INCR + EXPIREAT per window
```

| Endpoint Group | Default RPM | Header on Exceed |
|----------------|-------------|------------------|
| `retrieve` | 60 | `Retry-After: <seconds>` |
| `ingest` | 300 | `Retry-After: <seconds>` |
| `api` | 120 | `Retry-After: <seconds>` |

All limits configurable per workspace in `workspace.config`. Exceeding returns `429 Too Many Requests`.

---

## 21. Frontend Architecture

### 21.1 Stack

| Concern | Choice | Reason |
|---------|--------|---------|
| Framework | React 19 + TypeScript | Stable, wide ecosystem |
| Build | Vite | Fast HMR, ESM-native |
| State | Zustand | Minimal, no boilerplate |
| Data fetching | TanStack Query | Cache + stale-while-revalidate |
| API client | Generated from `openapi.yaml` via `openapi-typescript-codegen` | Single source of truth |
| Styling | Tailwind CSS | Utility-first, consistent |
| Components | shadcn/ui | Accessible, composable |
| Tables | TanStack Table | Powerful, headless |
| Charts | Recharts | Lightweight, declarative |
| Testing | Vitest + React Testing Library | Fast, co-located |
| E2E | Playwright | Cross-browser |

### 21.2 API Client

The frontend **never** hand-writes API calls. The client is generated from `docs/openapi.yaml`:

```bash
npx openapi-typescript-codegen \
  --input docs/openapi.yaml \
  --output frontend/src/api \
  --client axios
```

Regenerated in CI on any change to `openapi.yaml`. PRs that change the API without updating `openapi.yaml` fail CI.

### 21.3 Views

| Route | Component | Description |
|-------|-----------|-------------|
| `/` | Dashboard | Workspace overview, ingestion rate, memory counts |
| `/memory` | MemoryExplorer | Search + filter + bulk actions |
| `/memory/:id` | MemoryDetail | Full content, entities, lineage, version history |
| `/retrieve` | RetrievePlayground | Test retrieval queries interactively |
| `/retrieve/trace/:id` | RetrievalTrace | Score breakdown, inclusion/exclusion reasons |
| `/lifecycle` | LifecycleView | Episodic → semantic promotion timeline |
| `/integrations` | IntegrationStatus | Webhook health, DLQ, ingestion rate per source |
| `/audit` | AuditLog | All state-changing operations |
| `/settings` | WorkspaceSettings | Config, scoring weights, provider selection, API keys |

### 21.4 State Management

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

### 21.5 Component Rules

- No business logic in components — logic lives in custom hooks
- All data fetching in hooks, not components
- Components receive data as props or read from hooks — no direct API calls
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
    pub target_type: String,     // "memory" | "api_key" | "workspace" | "integration"
    pub diff: Option<serde_json::Value>,  // {before: {...}, after: {...}} for edits
    pub occurred_at: DateTime<Utc>,
}

#[derive(Debug, Clone, sqlx::Type)]
#[sqlx(type_name = "audit_action", rename_all = "snake_case")]
pub enum AuditAction {
    MemoryCreated,
    MemoryEdited,
    MemoryDeleted,
    MemoryRestored,
    MemoryPinned,
    MemoryUnpinned,
    MemoryPromoted,
    MemoryMerged,
    ImportanceOverridden,
    KeyCreated,
    KeyRevoked,
    WorkspaceConfigUpdated,
    IntegrationAdded,
    IntegrationRemoved,
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

#[derive(Debug, sqlx::Type)]
#[sqlx(type_name = "integration_status", rename_all = "lowercase")]
pub enum IntegrationStatus {
    Active,    // error rate < 5%
    Degraded,  // error rate 5–50%
    Failing,   // error rate > 50%
}
```

### 23.2 Dead Letter Queue

- Backend: Redis list `dlq:{workspace_id}`
- Entry schema: `{ job_id, payload, error, retry_count, failed_at }`
- Auto-retry: 3 attempts with exponential backoff (1s, 4s, 16s)
- After 3 failures: written to DLQ, integration `error_count_24h` incremented
- Manual retry via `POST /v1/workspaces/:id/dlq/:job_id/retry`
- DLQ entries expire after 7 days

---

## 24. Export & Backup

- `GET /v1/workspaces/:id/export` — streams JSONL, one memory unit per line
- Uses chunked transfer encoding — no in-memory buffer for large workspaces
- Includes: `content, entities, importance_score, tags, scope, memory_type, source_events, created_at`
- Excludes: `payload` from raw events (may contain PII), embeddings (raw vectors)
- Response header: `Content-Disposition: attachment; filename="memoryops-export-{workspace_id}-{date}.jsonl"`
- Import: planned for v0.3

---

## 25. Code Quality Standards

### 25.1 Rust

- **Format:** `rustfmt` with project `.rustfmt.toml`; enforced in CI
- **Lint:** `clippy` with `-D warnings`; all warnings treated as errors in CI
- **No `unwrap()`/`expect()`** outside tests — use `?` or map to `AppError`
- **No `clone()`** on large structs in hot paths — prefer `Arc<T>` for shared state
- **Derive traits explicitly:** `Debug, Clone, Serialize, Deserialize` only where needed
- **Dead code:** `#[allow(dead_code)]` is forbidden — remove or use the code
- **Unsafe:** `unsafe` blocks are forbidden unless in a dedicated `unsafe.rs` with a safety comment
- **Dependencies:** every new dep requires a comment explaining why it was chosen over alternatives
- **MSRV:** Rust stable, pinned in `rust-toolchain.toml`

### 25.2 TypeScript

- **Strict mode:** `"strict": true` in `tsconfig.json` — no implicit `any`
- **Format:** Prettier with project config; enforced in CI
- **Lint:** ESLint with `@typescript-eslint` + `react-hooks` rules
- **No `any`:** Use `unknown` + type guards instead
- **No direct API calls in components** — all in hooks
- **All async functions:** must handle errors — no unhandled promise rejections

### 25.3 SQL

- All queries use `sqlx::query_as!` macro for compile-time checking
- No raw string interpolation in queries — ever
- Explain-analyze run on all new queries before merge (added as PR comment)
- Migrations tested in integration suite before merge

### 25.4 Git Hygiene

- PR must reference an issue or milestone
- Each PR does one thing — no bundling unrelated changes
- PR description must include: what changed, why, how to test
- Squash merge to main — clean linear history
- No force-push to `main`

---

## 26. Non-Goals (v0.1)

- Generic "second brain" or consumer product
- Full agent runtime or framework
- Vector database replacement
- Real-time streaming ingestion (webhooks are sufficient)
- Custom embedding model fine-tuning
- Push-mode context injection (agent SDK — v0.3+)
- SSO / OAuth login (v0.3+ SaaS phase)
- Multi-region deployment
- Import from export (v0.3)

---

## 27. Risks & Mitigations

| Risk | Likelihood | Impact | Mitigation |
|------|-----------|--------|------------|
| LLM latency blocks throughput | Medium | High | Async-only slow path; local Ollama default |
| Large context windows reduce retrieval need | Low | Medium | Moat is lifecycle + governance, not retrieval size |
| Retrieval commoditized by frameworks | Medium | Medium | Differentiator is ingestion + control UI + trace |
| GitHub/Slack API changes break parsers | Low | High | Versioned event types; parser tests against fixtures |
| Qdrant unavailability degrades retrieval | Low | High | Fallback to BM25-only (Tantivy) if Qdrant unreachable |
| Webhook replay attacks | Low | Medium | Idempotency key + timestamp validation (5min window) |
| Postgres migration failure in prod | Low | Critical | Always run migrations in staging first; rollback plan per migration |

---

## 28. Milestones

| # | Deliverable | Key Acceptance Criteria | Target |
|---|-------------|------------------------|--------|
| M1 | Rust workspace + Cargo.toml + docker-compose + migrations scaffold | `cargo build` clean; `docker compose up` starts all infra | Week 1 |
| M2 | GitHub webhook ingestion + RawEvent writes | Webhook delivers → `raw_events` row exists; idempotency key works | Week 2 |
| M3 | Fast path processor + episodic MemoryUnit | Ingest PR → MemoryUnit with entities + importance created | Week 3 |
| M4 | Slow path worker + embeddings + Qdrant | MemoryUnit gets `embedding_id`; Qdrant point queryable | Week 4 |
| M5 | Promotion pipeline | Episodic cluster → Semantic MemoryUnit after threshold | Week 5 |
| M6 | Retrieval engine | `POST /retrieve` returns scored, token-packed, traced result | Week 6 |
| M7 | Full REST API + auth + rate limiting + audit | All endpoints passing integration tests; 401/429 enforced | Week 7 |
| M8 | React Memory Control Center | Explorer, detail, trace views functional against real API | Week 8 |
| M9 | Slack ingestion | Slack message → MemoryUnit via same pipeline | Week 9 |
| M10 | Demo | Agent failure → memory fix → trace walkthrough recorded | Week 10 |
