-- sqlx-disable-transaction

ALTER TYPE audit_action ADD VALUE IF NOT EXISTS 'user_erasure';

CREATE TABLE IF NOT EXISTS compliance_audit_log (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    workspace_id    UUID NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    action          TEXT NOT NULL,
    target_user_id  TEXT,
    memories_purged INTEGER NOT NULL DEFAULT 0,
    raw_events_purged INTEGER NOT NULL DEFAULT 0,
    initiated_by    TEXT NOT NULL,
    notes           TEXT,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_compliance_audit_workspace
    ON compliance_audit_log (workspace_id, created_at DESC);

CREATE INDEX idx_compliance_audit_action
    ON compliance_audit_log (workspace_id, action, created_at DESC);