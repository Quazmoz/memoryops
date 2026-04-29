CREATE TABLE workspace_skills (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    workspace_id    UUID NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    name            TEXT NOT NULL,
    description     TEXT NOT NULL,
    endpoint_url    TEXT NOT NULL,
    http_method     TEXT NOT NULL DEFAULT 'POST',
    input_schema    JSONB NOT NULL DEFAULT '{}',
    output_schema   JSONB NOT NULL DEFAULT '{}',
    auth_header     TEXT,
    auth_secret_enc TEXT,
    enabled         BOOL NOT NULL DEFAULT TRUE,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(workspace_id, name),
    CHECK (http_method IN ('GET', 'POST', 'PUT'))
);

CREATE TRIGGER trg_workspace_skills_updated_at
    BEFORE UPDATE ON workspace_skills
    FOR EACH ROW EXECUTE FUNCTION set_updated_at();

CREATE INDEX workspace_skills_workspace_id_enabled
    ON workspace_skills(workspace_id) WHERE enabled = TRUE;
