-- Add an audit action for explicit workspace tool secret reveal operations.
ALTER TYPE audit_action ADD VALUE IF NOT EXISTS 'tool_secret_revealed';
