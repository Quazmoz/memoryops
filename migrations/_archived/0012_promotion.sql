-- sqlx-disable-transaction

ALTER TYPE audit_action ADD VALUE IF NOT EXISTS 'workspace.promote';

-- Semantic memory units (promoted from episodic clusters)
ALTER TABLE memory_units
  ADD COLUMN IF NOT EXISTS memory_type       TEXT NOT NULL DEFAULT 'episodic'
    CHECK (memory_type IN ('episodic', 'semantic')),
  ADD COLUMN IF NOT EXISTS promoted_at       TIMESTAMPTZ,
  ADD COLUMN IF NOT EXISTS source_episode_ids UUID[]        DEFAULT '{}',
  ADD COLUMN IF NOT EXISTS corroboration_count INTEGER      NOT NULL DEFAULT 1;

ALTER TABLE memory_units
  ALTER COLUMN memory_type SET DEFAULT 'episodic';

-- Per-workspace promotion threshold config (stored in workspace_config JSONB already;
-- add typed columns for easy querying)
ALTER TABLE workspaces
  ADD COLUMN IF NOT EXISTS promotion_threshold FLOAT8 NOT NULL DEFAULT 0.72,
  ADD COLUMN IF NOT EXISTS dedup_cosine_threshold FLOAT8 NOT NULL DEFAULT 0.92;

-- Index for batch promotion candidate scan
CREATE INDEX IF NOT EXISTS idx_memory_units_promotion_candidates
  ON memory_units (workspace_id, memory_type, decay_score)
  WHERE memory_type = 'episodic'
    AND deleted_at IS NULL
    AND embedding_id IS NOT NULL;

-- Index for deduplication lookup
CREATE INDEX IF NOT EXISTS idx_memory_units_semantic
  ON memory_units (workspace_id, memory_type)
  WHERE memory_type = 'semantic' AND deleted_at IS NULL;
