ALTER TABLE memory_units
    ADD COLUMN IF NOT EXISTS agent_id TEXT GENERATED ALWAYS AS (scope->>'agent_id') STORED,
    ADD COLUMN IF NOT EXISTS user_id TEXT GENERATED ALWAYS AS (scope->>'user_id') STORED,
    ADD COLUMN IF NOT EXISTS repo TEXT GENERATED ALWAYS AS (scope->>'repo') STORED;

-- Composite index for scope-filtered queries
CREATE INDEX IF NOT EXISTS idx_memory_units_scope_filter
  ON memory_units(workspace_id, agent_id, user_id, repo)
  WHERE deleted_at IS NULL;
