CREATE INDEX IF NOT EXISTS idx_memory_units_source_events
    ON memory_units USING GIN(source_events);

CREATE INDEX IF NOT EXISTS idx_memory_units_source_episode_ids
    ON memory_units USING GIN(source_episode_ids);

CREATE INDEX IF NOT EXISTS idx_audit_log_memory_lineage
    ON audit_log(workspace_id, target_type, target_id, occurred_at DESC)
    WHERE target_type = 'memory';
