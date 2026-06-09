-- Create agent_skill_versions table and add monotonic versioning to agent_skills

ALTER TABLE agent_skills
    ADD COLUMN version INT NOT NULL DEFAULT 1;

CREATE TABLE agent_skill_versions (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    agent_skill_id  UUID NOT NULL REFERENCES agent_skills(id) ON DELETE CASCADE,
    workspace_id    UUID NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    name            TEXT NOT NULL,
    version         INT NOT NULL,
    assistant       TEXT NOT NULL,
    title           TEXT NOT NULL,
    description     TEXT NOT NULL,
    instructions    TEXT NOT NULL,
    content         TEXT NOT NULL,
    change_note     TEXT,
    created_by      TEXT,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(agent_skill_id, version),
    CHECK (version >= 1),
    CHECK (assistant IN ('gemini', 'claude'))
);

CREATE INDEX agent_skill_versions_workspace_idx
    ON agent_skill_versions(workspace_id, assistant, name, version DESC);

-- Backfill v1 snapshots for existing agent skills
INSERT INTO agent_skill_versions (
    agent_skill_id, workspace_id, name, version, assistant, title,
    description, instructions, content, change_note, created_by, created_at
)
SELECT id, workspace_id, name, 1, assistant, title,
       description, instructions, content, 'backfilled initial version', NULL, created_at
FROM agent_skills
ON CONFLICT DO NOTHING;
