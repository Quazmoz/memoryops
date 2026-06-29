-- ============================================================================
-- 0002_audit.sql — Audit system: audit_log, compliance, outbox
-- ============================================================================
-- Consolidated from migrations 0007, 0023, 0026, 0027, 0033, 0044, 0045.

CREATE TABLE audit_log (
    id              UUID        PRIMARY KEY,
    workspace_id    UUID        NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    actor           TEXT        NOT NULL,
    action          audit_action NOT NULL,
    target_id       UUID        NOT NULL,
    target_type     TEXT        NOT NULL,
    diff            JSONB,

    -- Request / actor / target context (audit hardening)
    request_id      TEXT,
    correlation_id  TEXT,
    actor_type      TEXT,
    actor_id        TEXT,
    actor_display   TEXT,
    api_key_id      UUID,
    api_key_prefix  TEXT,
    source_ip       TEXT,
    user_agent      TEXT,
    route           TEXT,
    method          TEXT,
    status_code     INTEGER,
    reason          TEXT,
    target_name     TEXT,
    target_version  INTEGER,

    -- Classification / outcome
    severity        TEXT        NOT NULL DEFAULT 'info',
    category        TEXT,
    success         BOOLEAN     NOT NULL DEFAULT TRUE,
    error_code      TEXT,

    -- Structured (redacted) payloads
    before          JSONB,
    after           JSONB,
    metadata        JSONB,

    -- Tamper-evidence (HMAC hash chain, per workspace)
    seq             BIGINT,
    prev_hash       TEXT,
    hash            TEXT,

    occurred_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);

COMMENT ON COLUMN audit_log.seq IS
    'Per-workspace monotonic sequence number assigned at insert time for the tamper-evident hash chain. NULL for rows written before audit hardening.';
COMMENT ON COLUMN audit_log.hash IS
    'HMAC-SHA256 (hex) over the canonical row fields + prev_hash, keyed by AUDIT_SIGNING_KEY (or APP_SECRET_KEY). Tamper-evident, not tamper-proof.';
COMMENT ON COLUMN audit_log.prev_hash IS
    'Hash of the preceding audit row for this workspace, forming the chain. NULL or genesis sentinel for the first hashed row.';

CREATE TRIGGER trg_audit_log_updated_at
    BEFORE UPDATE ON audit_log
    FOR EACH ROW EXECUTE FUNCTION set_updated_at();

-- ── Compliance audit log ─────────────────────────────────────────────────────

CREATE TABLE compliance_audit_log (
    id                  UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    workspace_id        UUID        NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    action              TEXT        NOT NULL,
    target_user_id      TEXT,
    memories_purged     BIGINT      NOT NULL DEFAULT 0,
    raw_events_purged   BIGINT      NOT NULL DEFAULT 0,
    initiated_by        TEXT        NOT NULL,
    notes               TEXT,
    created_at          TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- ── Audit outbox (durable best-effort writes) ───────────────────────────────

CREATE TABLE audit_outbox (
    id              UUID        PRIMARY KEY,
    workspace_id    UUID        NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    payload         JSONB       NOT NULL,
    attempts        INTEGER     NOT NULL DEFAULT 0,
    last_error      TEXT,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    next_attempt_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- ── Indexes: audit_log ───────────────────────────────────────────────────────

CREATE INDEX idx_audit_log_workspace_time
    ON audit_log(workspace_id, occurred_at DESC);

CREATE INDEX idx_audit_log_workspace_action_time
    ON audit_log(workspace_id, action, occurred_at DESC);

CREATE INDEX idx_audit_log_memory_lineage
    ON audit_log(workspace_id, target_type, target_id, occurred_at DESC)
    WHERE target_type = 'memory';

CREATE INDEX idx_audit_log_workspace_time_id
    ON audit_log(workspace_id, occurred_at DESC, id DESC);

CREATE UNIQUE INDEX uq_audit_log_workspace_seq
    ON audit_log(workspace_id, seq)
    WHERE seq IS NOT NULL;

CREATE INDEX idx_audit_log_workspace_actor_time
    ON audit_log(workspace_id, actor, occurred_at DESC);

CREATE INDEX idx_audit_log_workspace_target
    ON audit_log(workspace_id, target_type, target_id, occurred_at DESC);

CREATE INDEX idx_audit_log_workspace_category_time
    ON audit_log(workspace_id, category, occurred_at DESC);

CREATE INDEX idx_audit_log_workspace_request
    ON audit_log(workspace_id, request_id)
    WHERE request_id IS NOT NULL;

CREATE INDEX idx_audit_log_workspace_correlation
    ON audit_log(workspace_id, correlation_id)
    WHERE correlation_id IS NOT NULL;

CREATE INDEX idx_audit_log_workspace_source_ip
    ON audit_log(workspace_id, source_ip)
    WHERE source_ip IS NOT NULL;

CREATE INDEX idx_audit_log_workspace_api_key
    ON audit_log(workspace_id, api_key_id)
    WHERE api_key_id IS NOT NULL;

-- ── Indexes: compliance_audit_log ────────────────────────────────────────────

CREATE INDEX idx_compliance_audit_workspace
    ON compliance_audit_log(workspace_id, created_at DESC);

CREATE INDEX idx_compliance_audit_action
    ON compliance_audit_log(workspace_id, action, created_at DESC);

-- ── Indexes: audit_outbox ────────────────────────────────────────────────────

CREATE INDEX idx_audit_outbox_due
    ON audit_outbox(next_attempt_at);
