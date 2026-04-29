-- M34: add audit_action enum values for contradiction resolution and workspace reindex.
-- ADD VALUE IF NOT EXISTS is idempotent and does not require a transaction block.
ALTER TYPE audit_action ADD VALUE IF NOT EXISTS 'contradiction_resolved';
ALTER TYPE audit_action ADD VALUE IF NOT EXISTS 'workspace_reindexed';
