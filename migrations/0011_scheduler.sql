-- sqlx-disable-transaction

ALTER TYPE audit_action ADD VALUE IF NOT EXISTS 'memory_embedded';
ALTER TYPE audit_action ADD VALUE IF NOT EXISTS 'memory_hard_deleted';

ALTER TABLE memory_units
    ADD COLUMN IF NOT EXISTS hard_deleted_at TIMESTAMPTZ;

CREATE INDEX IF NOT EXISTS idx_memory_units_pruning
    ON memory_units(decay_score)
    WHERE deleted_at IS NULL
      AND pinned = false
      AND importance_overridden = false;

CREATE INDEX IF NOT EXISTS idx_memory_units_hard_delete
    ON memory_units(deleted_at)
    WHERE deleted_at IS NOT NULL;
