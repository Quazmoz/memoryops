DROP INDEX IF EXISTS idx_raw_events_idempotency;

CREATE UNIQUE INDEX idx_raw_events_workspace_idempotency
    ON raw_events(workspace_id, idempotency_key);