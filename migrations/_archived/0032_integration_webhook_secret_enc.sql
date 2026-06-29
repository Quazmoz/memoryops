ALTER TABLE integrations
  ADD COLUMN IF NOT EXISTS webhook_secret_enc TEXT;

COMMENT ON COLUMN integrations.webhook_secret_enc IS
  'AES-GCM encrypted HMAC signing secret for per-workspace webhooks. Recreate integrations that only have legacy webhook_secret_hash values.';