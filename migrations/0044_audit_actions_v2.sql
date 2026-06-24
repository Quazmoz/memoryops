-- M44: extend audit_action with values required for production audit coverage.
-- `ALTER TYPE ... ADD VALUE IF NOT EXISTS` is idempotent. On PostgreSQL 12+ it
-- may run inside the migration transaction as long as the new value is not used
-- in the same transaction (we only declare them here; rows using them are written
-- by the application afterwards).
ALTER TYPE audit_action ADD VALUE IF NOT EXISTS 'workspace_created';
ALTER TYPE audit_action ADD VALUE IF NOT EXISTS 'workspace_bootstrap';
ALTER TYPE audit_action ADD VALUE IF NOT EXISTS 'integration_updated';
ALTER TYPE audit_action ADD VALUE IF NOT EXISTS 'integration_webhook_secret_changed';
ALTER TYPE audit_action ADD VALUE IF NOT EXISTS 'memory_imported';
ALTER TYPE audit_action ADD VALUE IF NOT EXISTS 'memory_exported';
ALTER TYPE audit_action ADD VALUE IF NOT EXISTS 'retrieval_feedback';
ALTER TYPE audit_action ADD VALUE IF NOT EXISTS 'contradiction_dismissed';
ALTER TYPE audit_action ADD VALUE IF NOT EXISTS 'auth_failed';
ALTER TYPE audit_action ADD VALUE IF NOT EXISTS 'audit_exported';
