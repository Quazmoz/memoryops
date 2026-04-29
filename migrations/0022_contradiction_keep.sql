-- Add keep_a / keep_b resolution modes to contradiction flags
ALTER TYPE contradiction_resolution ADD VALUE IF NOT EXISTS 'keep_a';
ALTER TYPE contradiction_resolution ADD VALUE IF NOT EXISTS 'keep_b';

-- Track which memory survived and which was archived when a winner is chosen
ALTER TABLE contradiction_flags
    ADD COLUMN IF NOT EXISTS kept_memory_id    UUID REFERENCES memory_units(id),
    ADD COLUMN IF NOT EXISTS discarded_memory_id UUID REFERENCES memory_units(id);

-- New audit actions for contradiction resolution and workspace re-index
ALTER TYPE audit_action ADD VALUE IF NOT EXISTS 'contradiction_resolved';
ALTER TYPE audit_action ADD VALUE IF NOT EXISTS 'workspace_reindexed';
