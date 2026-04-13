# MemoryOps — Technical Specification

**Version:** 0.1.0  
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

Current AI agents suffer from:

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

## 4. Core Data Model

### 4.1 RawEvent

Immutable record of an inbound event from any source.

```rust
pub struct RawEvent {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub source: Source,           // GitHub | Slack | Jira | Linear
    pub event_type: EventType,    // PR | Commit | Message | Review | Issue
    pub actor: String,            // user/login who triggered it
    pub payload: serde_json::Value,
    pub occurred_at: DateTime<Utc>,
    pub ingested_at: DateTime<Utc>,
}
```

### 4.2 MemoryUnit

The core product object. Created by the processor from one or more RawEvents.

```rust
pub struct MemoryUnit {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub memory_type: MemoryType,      // Episodic | Semantic
    pub content: String,              // distilled natural language
    pub entities: Vec<Entity>,        // people, repos, branches, topics
    pub importance_score: f32,        // 0.0 - 1.0
    pub source_events: Vec<Uuid>,     // lineage to RawEvents
    pub embedding: Option<Vec<f32>>,  // populated by async processor
    pub created_at: DateTime<Utc>,
    pub last_accessed_at: Option<DateTime<Utc>>,
    pub decay_score: f32,             // drives pruning scheduler
    pub pinned: bool,                 // user-locked, never decays
    pub tags: Vec<String>,
}
```

### 4.3 Memory Types

| Type | Description | Mutability | Example |
|------|-------------|------------|---------|
| `Episodic` | Raw time-indexed events | Immutable | "PR #42 opened by alice at 14:32" |
| `Semantic` | Distilled knowledge | Updateable | "Alice owns the auth module" |

### 4.4 Entity

```rust
pub struct Entity {
    pub entity_type: EntityType,  // Person | Repo | Branch | Topic | File
    pub value: String,
    pub confidence: f32,
}
```

---

## 5. System Architecture

### 5.1 Services

```
┌─────────────────────────────────────────────────────────────┐
│                        MemoryOps                            │
│                                                             │
│  ┌─────────────────┐     ┌──────────────────────────────┐   │
│  │  Ingestion Svc  │     │       Processor Svc          │   │
│  │                 │     │                              │   │
│  │  POST /webhook  │────▶│  Fast Path  │  Slow Path     │   │
│  │  /github        │     │  (sync)     │  (async/LLM)   │   │
│  │  /slack         │     │             │                │   │
│  └─────────────────┘     └──────────────────────────────┘   │
│           │                        │                        │
│           ▼                        ▼                        │
│       Postgres                 Redis Queue                  │
│       raw_events               processor_jobs              │
│           │                        │                        │
│           └───────────┬────────────┘                        │
│                       ▼                                     │
│              ┌─────────────────┐                            │
│              │  Retrieval Svc  │                            │
│              │                 │                            │
│              │  Qdrant (vec)   │                            │
│              │  Tantivy (BM25) │                            │
│              │  Scorer         │                            │
│              │  Token Packer   │                            │
│              └─────────────────┘                            │
│                       │                                     │
│              ┌─────────────────┐                            │
│              │    API Svc      │ ◀── Agent / UI requests    │
│              │  (axum)         │                            │
│              └─────────────────┘                            │
└─────────────────────────────────────────────────────────────┘
```

### 5.2 Crate Layout

```
crates/
├── common/          # Shared: types, DB pool, config, error types
├── ingestion/       # Webhook handlers, source-specific parsers
├── processor/       # Fast path worker, slow path LLM worker
├── retrieval/       # Scorer, hybrid search, token packer
└── api/             # axum router, request/response types
```

---

## 6. Ingestion Layer

### 6.1 GitHub Integration

**Webhook events handled:**
- `pull_request` — opened, closed, merged, reviewed
- `push` — commits with diffs
- `issue_comment` — discussions
- `pull_request_review` — review approvals/changes

**Normalization:**
Every GitHub event maps to a `RawEvent` with:
- `source: Source::GitHub`
- `actor` extracted from `sender.login`
- full payload preserved in `payload: serde_json::Value`
- `occurred_at` from event timestamp

### 6.2 Slack Integration

**Events handled:**
- `message` in tracked channels
- `reaction_added` on tracked messages
- Thread replies

### 6.3 Ingestion API

```
POST /webhooks/github     # GitHub webhook receiver
POST /webhooks/slack      # Slack Events API receiver
```

Both endpoints:
1. Validate HMAC signature
2. Deserialize payload
3. Write `RawEvent` to Postgres
4. Enqueue job to Redis
5. Return `202 Accepted` immediately

---

## 7. Processing Pipeline

### 7.1 Fast Path (Synchronous, No LLM)

Runs inline or as a lightweight worker. Handles:
- Entity extraction (regex + rules): people, repos, branches, file paths
- Tag assignment: `[pr, merge, hotfix, auth-module]`
- Importance scoring (rule-based):
  - Merge to main → high
  - PR review with changes requested → medium-high
  - Push to feature branch → low
- Write `MemoryUnit` (Episodic type, no embedding yet)

### 7.2 Slow Path (Async, LLM-backed)

Consumed from Redis queue. Handles:
- Summarization via LLM (e.g., OpenAI, local model)
- Semantic clustering — group related episodic memories
- Embedding generation → write to Qdrant
- Promotion evaluation: should this cluster become Semantic memory?

### 7.3 Promotion Pipeline

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
Importance Threshold Check  (score > 0.65?)
    │
    ▼
LLM Summarization
    │
    ▼
Semantic MemoryUnit  (stable, reusable, embedded)
```

---

## 8. Retrieval Engine

This is the **core differentiator**. The retrieval engine does not return "top K chunks." It returns an optimized context payload under a specified token budget.

### 8.1 Retrieval Request

```rust
pub struct RetrievalRequest {
    pub workspace_id: Uuid,
    pub query: String,
    pub token_budget: usize,          // e.g. 4096
    pub filters: Option<RetrievalFilters>,
}

pub struct RetrievalFilters {
    pub sources: Option<Vec<Source>>,
    pub memory_types: Option<Vec<MemoryType>>,
    pub entities: Option<Vec<String>>,
    pub since: Option<DateTime<Utc>>,
}
```

### 8.2 Scoring Formula

Every candidate `MemoryUnit` is scored before selection:

```
final_score = 
    (semantic_similarity  × 0.35)
  + (importance_score     × 0.25)
  + (recency_score        × 0.20)
  + (source_authority     × 0.10)
  + (memory_type_weight   × 0.10)
```

**Component definitions:**
- `semantic_similarity` — cosine similarity from Qdrant, normalized 0–1
- `importance_score` — assigned during processing, 0–1
- `recency_score` — `1 / (1 + days_since_event)`, decays over time
- `source_authority` — configurable per source; GitHub > Slack by default
- `memory_type_weight` — Semantic > Episodic (summaries preferred over raw logs)

### 8.3 Token Packing Algorithm

```
1. Candidate fetch: hybrid search (Qdrant + Tantivy), top 50 candidates
2. Score all candidates using formula above
3. Sort descending by final_score
4. Greedy pack:
   for each candidate (sorted):
     if (current_tokens + candidate.token_count) <= budget:
       include
     else:
       skip
5. Deduplication: drop candidates with >0.92 cosine similarity to already-included item
6. Return packed context + trace
```

### 8.4 Retrieval Response

```rust
pub struct RetrievalResult {
    pub memories: Vec<ScoredMemory>,
    pub total_tokens: usize,
    pub trace: Vec<RetrievalTraceEntry>,
}

pub struct ScoredMemory {
    pub memory: MemoryUnit,
    pub final_score: f32,
    pub token_count: usize,
}

pub struct RetrievalTraceEntry {
    pub memory_id: Uuid,
    pub score_breakdown: ScoreBreakdown,
    pub inclusion_reason: String,   // human-readable
    pub excluded: bool,
    pub exclusion_reason: Option<String>,
}
```

---

## 9. API Surface

### 9.1 Ingestion

| Method | Path | Description |
|--------|------|-------------|
| POST | `/webhooks/github` | GitHub webhook receiver |
| POST | `/webhooks/slack` | Slack Events API receiver |

### 9.2 Memory Management

| Method | Path | Description |
|--------|------|-------------|
| GET | `/memory` | List memory units (paginated, filterable) |
| GET | `/memory/:id` | Get single memory unit |
| PATCH | `/memory/:id` | Update (pin, tag, edit content) |
| DELETE | `/memory/:id` | Delete memory unit |
| POST | `/memory/:id/promote` | Manually promote episodic → semantic |

### 9.3 Retrieval

| Method | Path | Description |
|--------|------|-------------|
| POST | `/retrieve` | Core retrieval endpoint |
| GET | `/retrieve/trace/:query_id` | Get retrieval trace for a past query |

### 9.4 Workspace

| Method | Path | Description |
|--------|------|-------------|
| POST | `/workspaces` | Create workspace |
| GET | `/workspaces/:id` | Get workspace + integration status |
| POST | `/workspaces/:id/integrations` | Add GitHub/Slack integration |

---

## 10. Multi-Tenancy

- Every table includes `workspace_id UUID NOT NULL`
- All queries are scoped by `workspace_id` — enforced at the DB layer via `sqlx` query params, not application logic
- No cross-workspace data leakage possible at query level
- Workspace-level config: source authority weights, importance thresholds, token budget defaults

---

## 11. Decay & Pruning

- Every `MemoryUnit` has a `decay_score: f32` (starts at 1.0)
- A scheduled job runs daily:
  - Decrements `decay_score` based on age + access frequency
  - Pinned memories are excluded
  - Semantic memories decay slower than Episodic
- Pruning threshold: `decay_score < 0.1` → archived (soft delete)
- Archival is recoverable via UI for 30 days

---

## 12. Frontend — Memory Control Center

### 12.1 Views

| View | Description |
|------|-------------|
| Memory Explorer | Searchable, filterable list of all memory units |
| Memory Detail | Full content, entities, score, source lineage |
| Retrieval Trace | For any past query: what was returned, why, what was excluded |
| Lifecycle View | Timeline of episodic → semantic promotions |
| Integration Status | Webhook health, ingestion rate, last event per source |

### 12.2 Memory Actions

- **Pin** — lock memory, prevent decay
- **Delete** — remove immediately
- **Merge** — combine two semantic memories
- **Edit** — manually correct content
- **Promote** — force episodic → semantic

---

## 13. Non-Goals (v0.1)

- Generic "second brain" or consumer use case
- Full agent framework or agent runtime
- Vector DB replacement (Qdrant is used, not replaced)
- Real-time streaming ingestion (webhook batch is sufficient for v0.1)
- Custom embedding model training

---

## 14. Risks & Mitigations

| Risk | Mitigation |
|------|------------|
| LLM cost/latency in slow path | Async-only; fast path never blocks on LLM |
| Large context windows reducing need | Focus on lifecycle + governance, not just retrieval size |
| Retrieval commoditization | Moat is in ingestion + lifecycle + control UI, not raw search |
| Integration complexity | Start with GitHub only; Slack in v0.2 |
| Crowded vector DB space | Explicitly not a vector DB — different positioning |

---

## 15. Milestones

| Milestone | Deliverable | Target |
|-----------|-------------|--------|
| M1 | Rust workspace + GitHub ingestion + Postgres writes | Week 2 |
| M2 | Fast path processor + episodic memory units | Week 3 |
| M3 | Slow path worker + embeddings + Qdrant writes | Week 4 |
| M4 | Retrieval engine (scoring + token packing) | Week 6 |
| M5 | REST API complete + multi-tenant enforcement | Week 6 |
| M6 | React Memory Control Center (explorer + trace) | Week 8 |
| M7 | Slack ingestion | Week 9 |
| M8 | Demo-ready: agent failure → memory fix → trace | Week 10 |
