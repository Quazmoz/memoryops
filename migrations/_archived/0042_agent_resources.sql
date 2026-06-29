-- Versioned agent resource library.
--
-- Agent skills remain available through the legacy agent_skills table/API, but
-- this table is the canonical versioned registry for skills, agents, prompts,
-- and reusable instructions.

CREATE TABLE agent_resources (
    id           UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    workspace_id UUID NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    kind         TEXT NOT NULL,
    assistant    TEXT NOT NULL,
    name         TEXT NOT NULL,
    filename     TEXT NOT NULL,
    title        TEXT NOT NULL,
    description  TEXT NOT NULL,
    body         TEXT NOT NULL,
    content      TEXT NOT NULL,
    metadata     JSONB NOT NULL DEFAULT '{}',
    version      INT NOT NULL DEFAULT 1,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at   TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(workspace_id, kind, assistant, name),
    CHECK (kind IN ('skill', 'agent', 'prompt', 'instruction')),
    CHECK (assistant IN ('generic', 'openai', 'claude', 'gemini')),
    CHECK (version >= 1)
);

CREATE TABLE agent_resource_versions (
    id           UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    resource_id  UUID NOT NULL REFERENCES agent_resources(id) ON DELETE CASCADE,
    workspace_id UUID NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    kind         TEXT NOT NULL,
    assistant    TEXT NOT NULL,
    name         TEXT NOT NULL,
    filename     TEXT NOT NULL,
    title        TEXT NOT NULL,
    description  TEXT NOT NULL,
    body         TEXT NOT NULL,
    content      TEXT NOT NULL,
    metadata     JSONB NOT NULL DEFAULT '{}',
    version      INT NOT NULL,
    change_note  TEXT,
    created_by   TEXT,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(resource_id, version),
    CHECK (kind IN ('skill', 'agent', 'prompt', 'instruction')),
    CHECK (assistant IN ('generic', 'openai', 'claude', 'gemini')),
    CHECK (version >= 1)
);

CREATE TRIGGER trg_agent_resources_updated_at
    BEFORE UPDATE ON agent_resources
    FOR EACH ROW EXECUTE FUNCTION set_updated_at();

CREATE INDEX agent_resources_workspace_kind_idx
    ON agent_resources(workspace_id, kind, assistant, LOWER(title));

CREATE INDEX agent_resource_versions_workspace_kind_name_version_idx
    ON agent_resource_versions(workspace_id, kind, assistant, name, version DESC);

-- Backfill the existing database-backed agent skills into the versioned
-- registry so every existing skill starts with a v1 snapshot.
INSERT INTO agent_resources (
    workspace_id, kind, assistant, name, filename, title, description,
    body, content, version, created_at, updated_at
)
SELECT workspace_id, 'skill', assistant, name, filename, title, description,
       instructions, content, 1, created_at, updated_at
FROM agent_skills
ON CONFLICT (workspace_id, kind, assistant, name) DO NOTHING;

INSERT INTO agent_resource_versions (
    resource_id, workspace_id, kind, assistant, name, filename, title,
    description, body, content, version, change_note, created_by, created_at
)
SELECT id, workspace_id, kind, assistant, name, filename, title,
       description, body, content, 1, 'backfilled initial version', NULL, created_at
FROM agent_resources
WHERE kind = 'skill'
ON CONFLICT (resource_id, version) DO NOTHING;

ALTER TYPE audit_action ADD VALUE IF NOT EXISTS 'agent_resource_created';
ALTER TYPE audit_action ADD VALUE IF NOT EXISTS 'agent_resource_updated';
ALTER TYPE audit_action ADD VALUE IF NOT EXISTS 'agent_resource_deleted';
ALTER TYPE audit_action ADD VALUE IF NOT EXISTS 'agent_resource_rolled_back';
