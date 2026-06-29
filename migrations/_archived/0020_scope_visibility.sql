-- sqlx-disable-transaction

ALTER TYPE audit_action ADD VALUE IF NOT EXISTS 'publish';

ALTER TABLE memory_units
    ADD COLUMN IF NOT EXISTS scope_visibility VARCHAR(16)
        NOT NULL DEFAULT 'private'
        CHECK (scope_visibility IN ('private', 'workspace'));

CREATE INDEX IF NOT EXISTS idx_memory_units_scope_visibility
    ON memory_units (workspace_id, scope_visibility)
    WHERE deleted_at IS NULL AND memory_type = 'semantic';
