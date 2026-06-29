-- M32: agent observation ingest — enum values only.
-- ADD VALUE IF NOT EXISTS is idempotent but cannot be used in the same transaction
-- as indexes that reference the new value; indexes live in 0025.
ALTER TYPE source ADD VALUE IF NOT EXISTS 'observation';
ALTER TYPE event_type ADD VALUE IF NOT EXISTS 'agent_observation';
ALTER TYPE audit_action ADD VALUE IF NOT EXISTS 'observation_ingested';
