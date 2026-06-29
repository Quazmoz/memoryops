-- ============================================================================
-- 0007_operations.sql — Operational and admin tables
-- ============================================================================
-- Consolidated from migrations 0031, 0046.

-- Track background deletion of workspace-associated data (memories,
-- embeddings, audit logs, etc.) after a workspace is soft-deleted.
CREATE TABLE workspace_purge_jobs (
    id            UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    workspace_id  UUID        NOT NULL,
    status        TEXT        NOT NULL DEFAULT 'pending'
                                  CHECK (status IN ('pending', 'running', 'done', 'failed')),
    started_at    TIMESTAMPTZ,
    finished_at   TIMESTAMPTZ,
    error         TEXT,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at    TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Singleton row for root admin credentials (bootstrapped on first start).
CREATE TABLE app_admin_credentials (
    id                  BOOLEAN     PRIMARY KEY DEFAULT TRUE CHECK (id),
    root_password_hash  TEXT        NOT NULL,
    root_password_enc   TEXT        NOT NULL,
    created_at          TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at          TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- ── Triggers ─────────────────────────────────────────────────────────────────

CREATE TRIGGER trg_app_admin_credentials_updated_at
    BEFORE UPDATE ON app_admin_credentials
    FOR EACH ROW EXECUTE FUNCTION set_updated_at();

-- ── Indexes ──────────────────────────────────────────────────────────────────

CREATE INDEX idx_workspace_purge_jobs_workspace_id
    ON workspace_purge_jobs(workspace_id);

CREATE INDEX idx_workspace_purge_jobs_status
    ON workspace_purge_jobs(status)
    WHERE status IN ('pending', 'running');
