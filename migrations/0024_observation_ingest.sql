-- M32: agent observation ingest.
-- ADD VALUE IF NOT EXISTS is idempotent and does not require a transaction block.
ALTER TYPE source ADD VALUE IF NOT EXISTS 'observation';
ALTER TYPE event_type ADD VALUE IF NOT EXISTS 'agent_observation';
ALTER TYPE audit_action ADD VALUE IF NOT EXISTS 'observation_ingested';

-- Fast lookup for observation-source stats and audit queries.
CREATE INDEX IF NOT EXISTS idx_raw_events_observation
    ON raw_events (workspace_id, ingested_at DESC)
    WHERE source = 'observation';

-- Agent observation feed: filter memories whose scope marks them as observations.
CREATE INDEX IF NOT EXISTS idx_memory_units_observation_agent
    ON memory_units ((scope->>'agent_id'), created_at DESC)
    WHERE scope->>'source' = 'observation';
