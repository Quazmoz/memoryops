-- Skill versioning: monotonic version on workspace_skills + immutable history table.

ALTER TABLE workspace_skills
    ADD COLUMN version INT NOT NULL DEFAULT 1;

CREATE TABLE workspace_skill_versions (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    skill_id        UUID NOT NULL REFERENCES workspace_skills(id) ON DELETE CASCADE,
    workspace_id    UUID NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    name            TEXT NOT NULL,
    version         INT  NOT NULL,
    description     TEXT NOT NULL,
    endpoint_url    TEXT NOT NULL,
    http_method     TEXT NOT NULL,
    input_schema    JSONB NOT NULL,
    output_schema   JSONB NOT NULL,
    auth_header     TEXT,
    auth_secret_enc TEXT,
    enabled         BOOL NOT NULL,
    change_note     TEXT,
    created_by      TEXT,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(skill_id, version),
    CHECK (version >= 1),
    CHECK (http_method IN ('GET', 'POST', 'PUT'))
);

CREATE INDEX workspace_skill_versions_workspace_name_version_idx
    ON workspace_skill_versions(workspace_id, name, version DESC);

-- Backfill v1 snapshots for existing skills so history is contiguous.
INSERT INTO workspace_skill_versions (
    skill_id, workspace_id, name, version, description, endpoint_url,
    http_method, input_schema, output_schema, auth_header, auth_secret_enc,
    enabled, change_note, created_by, created_at
)
SELECT id, workspace_id, name, 1, description, endpoint_url,
       http_method, input_schema, output_schema, auth_header, auth_secret_enc,
       enabled, 'backfilled initial version', NULL, created_at
FROM workspace_skills
ON CONFLICT DO NOTHING;
