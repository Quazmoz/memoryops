-- Index to support as_of filtering efficiently
CREATE INDEX IF NOT EXISTS idx_memory_units_created_at
    ON memory_units (workspace_id, created_at)
    WHERE deleted_at IS NULL;

CREATE INDEX IF NOT EXISTS idx_memory_versions_memory_as_of
    ON memory_versions (memory_id, created_at DESC);
