ALTER TABLE memory_units ADD COLUMN IF NOT EXISTS access_count INTEGER NOT NULL DEFAULT 0;

CREATE INDEX IF NOT EXISTS idx_memory_units_fts
    ON memory_units USING GIN (to_tsvector('english', content));

CREATE INDEX IF NOT EXISTS idx_memory_units_workspace_type_score
    ON memory_units (workspace_id, memory_type, importance_score DESC)
    WHERE deleted_at IS NULL;

CREATE INDEX IF NOT EXISTS idx_memory_units_workspace_pinned
    ON memory_units (workspace_id, pinned, updated_at DESC)
    WHERE deleted_at IS NULL AND pinned = true;