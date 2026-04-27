ALTER TABLE workspaces
    ADD COLUMN IF NOT EXISTS config JSONB NOT NULL DEFAULT '{}'::jsonb,
    ADD COLUMN IF NOT EXISTS deleted_at TIMESTAMPTZ;

CREATE INDEX IF NOT EXISTS idx_workspaces_active
    ON workspaces(id)
    WHERE deleted_at IS NULL;