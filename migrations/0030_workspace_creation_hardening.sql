ALTER TABLE workspaces
    ADD COLUMN IF NOT EXISTS created_from_ip INET;

ALTER TYPE audit_action ADD VALUE IF NOT EXISTS 'workspace_deleted';
