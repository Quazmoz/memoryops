# Audit Log

**Status:** Production
**Applies to:** MemoryOps API (`audit_log`, `audit_outbox`)

MemoryOps treats audit logging as a security control, not optional telemetry.
Security- and compliance-sensitive operations are recorded with **reliable**
write semantics — they are never silently dropped — and every payload is
redacted before it touches the database.

---

## 1. Reliability model

There are two write paths with deliberately different guarantees.

### Required (reliable) events

Written **synchronously** in the request path via `write_audit`. If the audit
row cannot be persisted, the operation fails (or, for already-committed
irreversible operations, the event is parked in the durable outbox). A
security-sensitive action never "succeeds silently" without an audit record.

For the most sensitive reads — e.g. **revealing a tool secret** — the audit row
is committed *before* the secret is returned, so a secret is never disclosed
without a durable trail.

Required events include: API key created/revoked, workspace
created/deleted/config-updated, integration added/updated/removed, webhook
secret changed, tool created/updated/deleted/rolled-back, tool secret revealed,
agent resource created/updated/deleted/rolled-back, user erasure, memory
hard-delete, and audit export.

### Best-effort (operational) events

Written **asynchronously** via `spawn_audit_event` for high-volume operational
events (memory embedding, observation ingest, tool invocation, scheduler
maintenance, promotion, reindex). These do **not** block the request.

Crucially, best-effort writes are **not dropped on a full queue** (the previous
behavior). Writers await a bounded concurrency permit, and if a direct write
fails the redacted event is enqueued into the durable `audit_outbox` table. A
background drainer retries the outbox every 60 seconds. Undecodable rows are
dropped loudly after logging; transient failures back off and retry.

`AuditAction::is_required()` is the single source of truth for which events use
which path, and the `/audit/actions` endpoint exposes the `required` flag per
action.

### Call-site classification

Every audit-emitting call site is classified as required, operational, or
high-volume telemetry. Required events use `write_audit` /
`write_audit_in_conn`; the rest use the best-effort path (which still falls back
to the durable outbox on failure — never silently dropped).

| Call site | Action(s) | Class | Path |
| --- | --- | --- | --- |
| `api/handlers/keys.rs` | `key_created/revoked` | required | `write_audit` |
| `api/handlers/workspaces.rs` | `workspace_created/deleted`, `config_updated` | required | `write_audit` |
| `api/handlers/integrations.rs` | `integration_*`, `webhook_secret_changed` | required | `write_audit` |
| `api/handlers/tools.rs` | `tool_*`, `tool_secret_revealed` | required | `write_audit` |
| `api/handlers/agent_resources.rs` | `agent_resource_*` | required | `write_audit` |
| `api/handlers/agent_skills.rs` (legacy) | `agent_resource_created/updated` | required | `write_audit` (migrated) |
| `api/handlers/compliance.rs` | `user_erasure` | required | `write_audit` |
| `api/handlers/audit.rs` | `audit_exported` | required | `write_audit` |
| `processor/scheduler.rs` hard-delete | `memory_hard_deleted` | required | `write_audit_in_conn` (migrated; tx-atomic with the DELETE) |
| `processor/scheduler.rs` decay prune | `memory_deleted` | operational | best-effort |
| `retrieval/handlers/lifecycle.rs` | `memory_deleted/restored/promoted`, `publish` | operational | best-effort |
| `retrieval/handlers/update.rs` | `memory_edited/pinned/unpinned`, `importance_overridden` | operational | best-effort |
| `api/handlers/contradictions.rs` | `contradiction_resolved` | operational | best-effort |
| `processor/contradiction.rs` | flag detected (`memory_edited`) | operational | best-effort |
| `common/services/skills.rs` | `tool_invoked` | high-volume telemetry | best-effort |
| `ingestion/observation/ingest.rs` | `observation_ingested` | high-volume telemetry | best-effort |
| `processor/worker.rs` | `memory_embedded` | high-volume telemetry | best-effort |

**Intentionally best-effort.** Memory CRUD (`lifecycle.rs`/`update.rs`),
contradiction resolution, tool invocation, observation ingest, embedding, and
decay pruning are operational/high-volume events. They are not in
`is_required()` and use the best-effort path on purpose: they are non-reversible
or self-healing, are emitted at high volume, and must not add latency or fail
the originating operation. The durable outbox still guarantees they are not lost
on a transient write failure. Only `memory_hard_deleted` and the legacy
agent-skills writes were promoted to the reliable path, because both are
`is_required()` compliance events.

---

## 2. What is audited

| Area | Actions |
| --- | --- |
| API keys | `key_created`, `key_revoked` |
| Workspace | `workspace_created`, `workspace_deleted`, `config_updated` / `workspace_config_updated`, `workspace_reindexed`, `workspace.promote` |
| Integrations | `integration_added`, `integration_updated`, `integration_removed`, `integration_webhook_secret_changed` |
| Tools | `tool_created`, `tool_updated`, `tool_deleted`, `tool_rolled_back`, `tool_invoked`, `tool_secret_revealed` |
| Agent resources | `agent_resource_created/updated/deleted/rolled_back` |
| Memory | `memory_created/edited/deleted/restored/promoted/merged/embedded/hard_deleted`, `publish`, `importance_overridden`, `memory_imported/exported` |
| Contradictions | `contradiction_resolved`, `contradiction_dismissed` |
| Retrieval | `retrieval_feedback` |
| Compliance | `user_erasure` |
| Security | `auth_failed`, `workspace_bootstrap`, `audit_exported` |
| Ingestion | `observation_ingested` |

Each record carries: actor (and `actor_type`/`actor_id`/`api_key_id`/
`api_key_prefix`), target (`target_type`/`target_id`/`target_name`/
`target_version`), `severity`, `category`, `success`, optional `reason`/
`error_code`, request context (`request_id`, `correlation_id`, `source_ip`,
`user_agent`, `method`, `route`), redacted `before`/`after`/`metadata`/`diff`,
and the hash-chain fields (`seq`, `prev_hash`, `hash`).

## 3. What is NOT audited

- Read-only list/get endpoints (except secret reveal and audit export).
- Full request/response bodies.
- Health checks and metrics scraping.
- Plaintext secret values of any kind (see redaction).

---

## 4. Secret safety & redaction

All JSON payloads (`before`, `after`, `metadata`, `diff`) are recursively
redacted **before persistence** and again on read (so even legacy rows are safe
through the API/export). A field is masked when its name contains any of:
`secret`, `token`, `key`, `password`, `credential`, `authorization`,
`auth_secret`, `auth_token`, `bearer`, `cookie`, `api_key`, `webhook_secret`,
`plaintext_secret`, `connection_string`, `database_url`, `private_key`,
`access_key`, `client_secret`, `session_token`.

```jsonc
// stored
{ "auth_secret": "[REDACTED]", "endpoint": "https://api.example.com" }
```

The audit diff therefore proves *that a secret changed* without storing the
value. Long strings become `{ "truncated": true, "length": N, "preview": "…" }`,
large arrays are capped, deep structures collapse, and any payload exceeding
32 KB is replaced by a size summary. Redaction is idempotent.

**Never stored:** auth headers' secret values, API keys, webhook secrets, tool
auth secrets, LLM provider keys, connection strings, or full request bodies.

---

## 5. Tamper-evidence

Each row is chained per workspace with an HMAC-SHA256 hash:

```
hash = HMAC(key, canonical(seq, workspace_id, id, prev_hash, occurred_at,
                            actor, action, target, success, severity, category,
                            before, after, metadata, diff))
```

`prev_hash` links to the previous row's hash (`GENESIS` for the first), forming
a chain. `seq` is a per-workspace monotonic sequence assigned under a
transaction-scoped advisory lock. Verify via:

```
POST /v1/workspaces/{id}/audit/verify
```

which recomputes every hashed row and reports the first broken sequence, if any.

**This is tamper-evident, not tamper-proof.** A party with both database write
access *and* the signing key can rewrite the chain. Mitigate by keeping
`AUDIT_SIGNING_KEY` off the database host and shipping audit data to
append-only/WORM storage.

### Signing key & rotation

- Set a dedicated `AUDIT_SIGNING_KEY` (preferred). If unset, the chain falls
  back to `APP_SECRET_KEY`. If neither is set, hashing is disabled (rows still
  write; `seq`/`hash` are NULL) and `verify` reports `enabled: false`.
- **Rotation implication:** rows written under the old key will no longer
  verify after rotation. Before rotating, export/archive the existing chain;
  treat the rotation point as a chain boundary. Prefer `AUDIT_SIGNING_KEY` over
  `APP_SECRET_KEY` precisely so audit signing and data-encryption keys can
  rotate independently.
- Hashing begins with rows written after the `0045_audit_hardening` migration;
  pre-existing rows are left untouched (NULL `seq`) and excluded from
  verification.

---

## 6. Query & export API

All endpoints are workspace-scoped and require a valid API key for the
workspace.

`GET /v1/workspaces/{id}/audit` — cursor-paginated list, stable ordering by
`(occurred_at, id)`. Filters: `actor`, `action`, `actions` (CSV),
`target_type`, `target_id`, `target_name`, `request_id`, `correlation_id`,
`source_ip`, `severity` (CSV), `category` (CSV), `success`, `from`/`to`/`since`,
and `q` (free-text over actor/target/request id/reason). `limit` (≤100) plus
`after` cursor; legacy `offset` is still accepted.

`GET /v1/workspaces/{id}/audit/{audit_id}` — single entry.

`GET /v1/workspaces/{id}/audit/actions` — catalog of actions with category,
default severity, and `required` flag, plus the severity/category vocabularies.

`GET /v1/workspaces/{id}/audit/export?format=jsonl|csv` — redacted export with
the same filters. Bounded to 50,000 rows; if the cap is hit the response carries
`x-audit-export-truncated: true` and you should narrow `from`/`to`. Exporting is
itself audited (`audit_exported`).

`POST /v1/workspaces/{id}/audit/verify` — hash-chain verification.

---

## 7. Investigation playbooks

- **Who changed this tool's endpoint?**
  `GET …/audit?target_type=workspace_tool&actions=tool_updated&q=<tool name>` —
  inspect the redacted `after`/`metadata` and `actor`.
- **Who revealed a tool secret?**
  `GET …/audit?actions=tool_secret_revealed&target_name=<tool>` — `actor`,
  `api_key_id`, `source_ip`, `occurred_at`.
- **What changed in workspace config?**
  `GET …/audit?actions=config_updated` then expand `before`/`after` (secrets
  shown as `[REDACTED]`).
- **Which API key deleted this memory?**
  `GET …/audit?actions=memory_deleted,memory_hard_deleted&target_id=<id>` →
  read `actor` / `api_key_id`.
- **What happened around this request id?**
  `GET …/audit?request_id=<rid>` to see every event from that request.

---

## 8. Retention & maintenance

- **Default: never prune.** With `AUDIT_RETENTION_DAYS` unset or `0`, audit
  history is preserved indefinitely. There is no destructive default.
- Set `AUDIT_RETENTION_DAYS > 0` to have the daily maintenance pass delete rows
  older than the window. Recommended: **local/dev** short or unset; **production**
  90–365 days depending on your compliance regime.
- **Export/archive before enabling retention.** Pruning is irreversible. The
  recommended flow is: scheduled export (JSONL) to object storage → verify →
  then prune.
- The outbox drainer runs every 60s; the retention pass runs in the daily
  maintenance window.

### Backup recommendations

- Include `audit_log` (and `audit_outbox`) in regular Postgres backups.
- For high-assurance environments, stream exports to append-only / WORM storage
  and keep the `AUDIT_SIGNING_KEY` separate so the chain can be independently
  verified against the archive.

---

## 9. Compliance notes

- Audit records are workspace-scoped; cross-workspace reads are not possible
  through the API.
- `user_erasure` is recorded (who, mode, counts) without storing the erased
  content, complementing the dedicated `compliance_audit_log`.
- Retention deletion is a deliberate, configured action — document your chosen
  `AUDIT_RETENTION_DAYS` in your data-retention policy, balancing minimization
  against investigative/forensic needs.

---

## 10. Configuration summary

| Variable | Default | Purpose |
| --- | --- | --- |
| `AUDIT_SIGNING_KEY` | falls back to `APP_SECRET_KEY` | HMAC key for the tamper-evident chain |
| `AUDIT_RETENTION_DAYS` | unset (no pruning) | Days of audit history to keep |
| `TRUSTED_PROXY_CIDRS` | empty | Peers trusted to set `X-Forwarded-For` for `source_ip` |
