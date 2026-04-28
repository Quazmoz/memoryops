ALTER TABLE integrations
  ADD COLUMN IF NOT EXISTS webhook_secret TEXT;

ALTER TABLE raw_events
  ADD COLUMN IF NOT EXISTS slack_channel TEXT,
  ADD COLUMN IF NOT EXISTS slack_thread_ts TEXT;

CREATE INDEX IF NOT EXISTS idx_raw_events_slack_channel
  ON raw_events(workspace_id, slack_channel)
  WHERE source = 'slack' AND slack_channel IS NOT NULL;
