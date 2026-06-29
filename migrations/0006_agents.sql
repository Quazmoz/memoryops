-- ============================================================================
-- 0006_agents.sql — Agent skills (legacy) and versioned agent resources
-- ============================================================================
-- Consolidated from migrations 0041, 0042.

-- Legacy agent skills table (retained for backward compatibility).
CREATE TABLE agent_skills (
    id           UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    workspace_id UUID        NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    name         TEXT        NOT NULL,
    filename     TEXT        NOT NULL,
    assistant    TEXT        NOT NULL,
    title        TEXT        NOT NULL,
    description  TEXT        NOT NULL,
    instructions TEXT        NOT NULL,
    content      TEXT        NOT NULL,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at   TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(workspace_id, assistant, name),
    CHECK (assistant IN ('gemini', 'claude'))
);

-- Versioned agent resource registry (canonical store for skills, agents,
-- prompts, and reusable instructions).
CREATE TABLE agent_resources (
    id           UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    workspace_id UUID        NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    kind         TEXT        NOT NULL,
    assistant    TEXT        NOT NULL,
    name         TEXT        NOT NULL,
    filename     TEXT        NOT NULL,
    title        TEXT        NOT NULL,
    description  TEXT        NOT NULL,
    body         TEXT        NOT NULL,
    content      TEXT        NOT NULL,
    metadata     JSONB       NOT NULL DEFAULT '{}',
    version      INT         NOT NULL DEFAULT 1,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at   TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(workspace_id, kind, assistant, name),
    CHECK (kind IN ('skill', 'agent', 'prompt', 'instruction')),
    CHECK (assistant IN ('generic', 'openai', 'claude', 'gemini')),
    CHECK (version >= 1)
);

CREATE TABLE agent_resource_versions (
    id           UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    resource_id  UUID        NOT NULL REFERENCES agent_resources(id) ON DELETE CASCADE,
    workspace_id UUID        NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    kind         TEXT        NOT NULL,
    assistant    TEXT        NOT NULL,
    name         TEXT        NOT NULL,
    filename     TEXT        NOT NULL,
    title        TEXT        NOT NULL,
    description  TEXT        NOT NULL,
    body         TEXT        NOT NULL,
    content      TEXT        NOT NULL,
    metadata     JSONB       NOT NULL DEFAULT '{}',
    version      INT         NOT NULL,
    change_note  TEXT,
    created_by   TEXT,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(resource_id, version),
    CHECK (kind IN ('skill', 'agent', 'prompt', 'instruction')),
    CHECK (assistant IN ('generic', 'openai', 'claude', 'gemini')),
    CHECK (version >= 1)
);

-- ── Triggers ─────────────────────────────────────────────────────────────────

CREATE TRIGGER trg_agent_resources_updated_at
    BEFORE UPDATE ON agent_resources
    FOR EACH ROW EXECUTE FUNCTION set_updated_at();

-- ── Indexes ──────────────────────────────────────────────────────────────────

CREATE INDEX agent_skills_workspace_idx
    ON agent_skills(workspace_id);

CREATE INDEX agent_resources_workspace_kind_idx
    ON agent_resources(workspace_id, kind, assistant, LOWER(title));

CREATE INDEX agent_resource_versions_workspace_kind_name_version_idx
    ON agent_resource_versions(workspace_id, kind, assistant, name, version DESC);
