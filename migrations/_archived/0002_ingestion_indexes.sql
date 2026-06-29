CREATE INDEX IF NOT EXISTS idx_raw_events_workspace_source_type
    ON raw_events(workspace_id, source, event_type, occurred_at DESC);