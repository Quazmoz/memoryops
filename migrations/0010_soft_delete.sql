ALTER TABLE memory_units
    ADD COLUMN IF NOT EXISTS deleted_at TIMESTAMPTZ;

CREATE INDEX IF NOT EXISTS idx_memory_units_workspace_active_updated
    ON memory_units(workspace_id, updated_at DESC)
    WHERE deleted_at IS NULL;

CREATE INDEX IF NOT EXISTS idx_memory_units_workspace_deleted
    ON memory_units(workspace_id, deleted_at DESC)
    WHERE deleted_at IS NOT NULL;