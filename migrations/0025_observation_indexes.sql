-- M32 cont: indexes that use the 'observation' enum value added in 0024.
-- Must be a separate migration because PostgreSQL does not allow using a newly
-- added enum value in the same transaction that added it.

-- Fast lookup for observation-source stats and audit queries.
CREATE INDEX IF NOT EXISTS idx_raw_events_observation
    ON raw_events (workspace_id, ingested_at DESC)
    WHERE source = 'observation';

-- Agent observation feed: filter memories whose scope marks them as observations.
CREATE INDEX IF NOT EXISTS idx_memory_units_observation_agent
    ON memory_units ((scope->>'agent_id'), created_at DESC)
    WHERE scope->>'source' = 'observation';
