ALTER TABLE integrations
  ALTER COLUMN webhook_secret_hash DROP NOT NULL;

COMMENT ON COLUMN integrations.webhook_secret IS
  'Plain HMAC signing secret for Slack, Linear, and Jira webhooks. Linear may be NULL for unsigned webhooks.';

COMMENT ON TYPE source IS
  'Webhook source enum. Values jira and linear were created in 0001_init.sql and are activated by M10.';

CREATE INDEX IF NOT EXISTS idx_integrations_active_source
  ON integrations(source, workspace_id)
  WHERE deleted_at IS NULL;

CREATE INDEX IF NOT EXISTS idx_raw_events_linear_jira_workspace_type
  ON raw_events(workspace_id, source, event_type, occurred_at DESC)
  WHERE source IN ('linear', 'jira');