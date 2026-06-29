-- sqlx-disable-transaction

ALTER TYPE audit_action ADD VALUE IF NOT EXISTS 'config_updated';

CREATE TABLE IF NOT EXISTS audit_log (
    id UUID PRIMARY KEY,
    workspace_id UUID NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    actor TEXT NOT NULL,
    action audit_action NOT NULL,
    target_id UUID NOT NULL,
    target_type TEXT NOT NULL,
    diff JSONB,
    occurred_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_audit_log_workspace_time
    ON audit_log(workspace_id, occurred_at DESC);

CREATE INDEX IF NOT EXISTS idx_audit_log_workspace_action_time
    ON audit_log(workspace_id, action, occurred_at DESC);