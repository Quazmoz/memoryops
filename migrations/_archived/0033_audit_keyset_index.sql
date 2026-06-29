CREATE INDEX IF NOT EXISTS idx_audit_log_workspace_time_id
    ON audit_log(workspace_id, occurred_at DESC, id DESC);

ALTER TABLE integrations
  ADD COLUMN IF NOT EXISTS api_token_enc TEXT,
  ADD COLUMN IF NOT EXISTS api_sync_enabled BOOLEAN NOT NULL DEFAULT FALSE,
  ADD COLUMN IF NOT EXISTS sync_config JSONB NOT NULL DEFAULT '{}'::jsonb,
  ADD COLUMN IF NOT EXISTS last_sync_at TIMESTAMPTZ;

COMMENT ON COLUMN integrations.api_token_enc IS
  'AES-GCM encrypted platform API token used by connector sync/backfill adapters.';

COMMENT ON COLUMN integrations.api_sync_enabled IS
  'Whether API-based connector sync/backfill is enabled for this integration.';

COMMENT ON COLUMN integrations.sync_config IS
  'Connector-specific sync settings such as selected repositories, channels, projects, cursors, or resource types.';

COMMENT ON COLUMN integrations.last_sync_at IS
  'Timestamp of the most recent API connector sync attempt for this integration.';