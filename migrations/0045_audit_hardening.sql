-- M45: production audit hardening.
--
-- Adds investigation/context columns, structured before/after/metadata payloads,
-- an HMAC hash chain for tamper-evidence, workspace-scoped investigation indexes,
-- and a durable outbox table so best-effort audit writes are never silently lost
-- when a transient failure occurs.
--
-- All changes are additive and idempotent. Existing audit rows keep working; the
-- new columns are nullable and the hash chain begins with rows written after this
-- migration is applied (older rows simply have NULL seq/hash).

-- ── Request / actor / target context ─────────────────────────────────────────
ALTER TABLE audit_log
    ADD COLUMN IF NOT EXISTS request_id      TEXT,
    ADD COLUMN IF NOT EXISTS correlation_id  TEXT,
    ADD COLUMN IF NOT EXISTS actor_type      TEXT,
    ADD COLUMN IF NOT EXISTS actor_id        TEXT,
    ADD COLUMN IF NOT EXISTS actor_display   TEXT,
    ADD COLUMN IF NOT EXISTS api_key_id      UUID,
    ADD COLUMN IF NOT EXISTS api_key_prefix  TEXT,
    ADD COLUMN IF NOT EXISTS source_ip       TEXT,
    ADD COLUMN IF NOT EXISTS user_agent      TEXT,
    ADD COLUMN IF NOT EXISTS route           TEXT,
    ADD COLUMN IF NOT EXISTS method          TEXT,
    ADD COLUMN IF NOT EXISTS status_code     INTEGER,
    ADD COLUMN IF NOT EXISTS reason          TEXT,
    ADD COLUMN IF NOT EXISTS target_name     TEXT,
    ADD COLUMN IF NOT EXISTS target_version  INTEGER;

-- ── Classification / outcome ─────────────────────────────────────────────────
ALTER TABLE audit_log
    ADD COLUMN IF NOT EXISTS severity   TEXT    NOT NULL DEFAULT 'info',
    ADD COLUMN IF NOT EXISTS category   TEXT,
    ADD COLUMN IF NOT EXISTS success    BOOLEAN NOT NULL DEFAULT TRUE,
    ADD COLUMN IF NOT EXISTS error_code TEXT;

-- ── Structured (redacted) payloads ───────────────────────────────────────────
-- `diff` is retained for backward compatibility; `before`/`after`/`metadata`
-- are the structured equivalents. All four are redacted + bounded before write.
ALTER TABLE audit_log
    ADD COLUMN IF NOT EXISTS before    JSONB,
    ADD COLUMN IF NOT EXISTS after     JSONB,
    ADD COLUMN IF NOT EXISTS metadata  JSONB;

-- ── Tamper-evidence (HMAC hash chain, per workspace) ─────────────────────────
ALTER TABLE audit_log
    ADD COLUMN IF NOT EXISTS seq        BIGINT,
    ADD COLUMN IF NOT EXISTS prev_hash  TEXT,
    ADD COLUMN IF NOT EXISTS hash       TEXT;

COMMENT ON COLUMN audit_log.seq IS
    'Per-workspace monotonic sequence number assigned at insert time for the tamper-evident hash chain. NULL for rows written before audit hardening.';
COMMENT ON COLUMN audit_log.hash IS
    'HMAC-SHA256 (hex) over the canonical row fields + prev_hash, keyed by AUDIT_SIGNING_KEY (or APP_SECRET_KEY). Tamper-evident, not tamper-proof.';
COMMENT ON COLUMN audit_log.prev_hash IS
    'Hash of the preceding audit row for this workspace, forming the chain. NULL or genesis sentinel for the first hashed row.';

-- Enforce chain integrity: at most one row per (workspace, seq).
CREATE UNIQUE INDEX IF NOT EXISTS uq_audit_log_workspace_seq
    ON audit_log(workspace_id, seq)
    WHERE seq IS NOT NULL;

-- ── Investigation indexes (all workspace-scoped) ─────────────────────────────
CREATE INDEX IF NOT EXISTS idx_audit_log_workspace_actor_time
    ON audit_log(workspace_id, actor, occurred_at DESC);

CREATE INDEX IF NOT EXISTS idx_audit_log_workspace_target
    ON audit_log(workspace_id, target_type, target_id, occurred_at DESC);

CREATE INDEX IF NOT EXISTS idx_audit_log_workspace_category_time
    ON audit_log(workspace_id, category, occurred_at DESC);

CREATE INDEX IF NOT EXISTS idx_audit_log_workspace_request
    ON audit_log(workspace_id, request_id)
    WHERE request_id IS NOT NULL;

CREATE INDEX IF NOT EXISTS idx_audit_log_workspace_correlation
    ON audit_log(workspace_id, correlation_id)
    WHERE correlation_id IS NOT NULL;

CREATE INDEX IF NOT EXISTS idx_audit_log_workspace_source_ip
    ON audit_log(workspace_id, source_ip)
    WHERE source_ip IS NOT NULL;

CREATE INDEX IF NOT EXISTS idx_audit_log_workspace_api_key
    ON audit_log(workspace_id, api_key_id)
    WHERE api_key_id IS NOT NULL;

-- ── Durable outbox for best-effort audit writes ──────────────────────────────
-- Best-effort audit events (high-volume operational events) are written directly
-- when possible; if the direct write fails they are enqueued here and retried by
-- a background drainer. Payloads stored here are already redacted + bounded, so
-- the outbox never holds secrets. Required (security-sensitive) audit events do
-- NOT use this path -- they are written synchronously and fail the operation if
-- the write fails.
CREATE TABLE IF NOT EXISTS audit_outbox (
    id              UUID PRIMARY KEY,
    workspace_id    UUID NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    payload         JSONB NOT NULL,
    attempts        INTEGER NOT NULL DEFAULT 0,
    last_error      TEXT,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    next_attempt_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_audit_outbox_due
    ON audit_outbox(next_attempt_at);
