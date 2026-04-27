CREATE TABLE IF NOT EXISTS api_keys (
    id UUID PRIMARY KEY,
    workspace_id UUID NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    key_hash TEXT NOT NULL,
    prefix TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    last_used_at TIMESTAMPTZ,
    revoked BOOLEAN NOT NULL DEFAULT false,
    revoked_at TIMESTAMPTZ,
    CONSTRAINT api_keys_prefix_len CHECK (char_length(prefix) = 8),
    CONSTRAINT api_keys_revoked_at_consistent CHECK (
        (revoked = false AND revoked_at IS NULL)
        OR (revoked = true AND revoked_at IS NOT NULL)
    )
);

CREATE INDEX IF NOT EXISTS idx_api_keys_workspace_prefix
    ON api_keys(workspace_id, prefix)
    WHERE revoked = false;

CREATE INDEX IF NOT EXISTS idx_api_keys_prefix
    ON api_keys(prefix);