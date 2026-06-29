CREATE TABLE IF NOT EXISTS integrations (
    workspace_id UUID NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    source source NOT NULL,
    webhook_secret_hash TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    deleted_at TIMESTAMPTZ,
    PRIMARY KEY (workspace_id, source)
);

DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_trigger WHERE tgname = 'trg_integrations_updated_at') THEN
        CREATE TRIGGER trg_integrations_updated_at
            BEFORE UPDATE ON integrations
            FOR EACH ROW EXECUTE FUNCTION set_updated_at();
    END IF;
END;
$$;

CREATE INDEX IF NOT EXISTS idx_integrations_active
    ON integrations(workspace_id, source)
    WHERE deleted_at IS NULL;