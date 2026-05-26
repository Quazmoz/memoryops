CREATE INDEX IF NOT EXISTS idx_audit_log_workspace_time_id
    ON audit_log(workspace_id, occurred_at DESC, id DESC);