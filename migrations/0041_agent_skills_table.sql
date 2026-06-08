-- Create agent_skills table for database-backed persistence.

CREATE TABLE agent_skills (
    id           UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    workspace_id UUID NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    name         TEXT NOT NULL,
    filename     TEXT NOT NULL,
    assistant    TEXT NOT NULL,
    title        TEXT NOT NULL,
    description  TEXT NOT NULL,
    instructions TEXT NOT NULL,
    content      TEXT NOT NULL,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at   TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(workspace_id, assistant, name),
    CHECK (assistant IN ('gemini', 'claude'))
);

CREATE INDEX agent_skills_workspace_idx ON agent_skills(workspace_id);
