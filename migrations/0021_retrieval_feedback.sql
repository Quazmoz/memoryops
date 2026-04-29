CREATE TABLE IF NOT EXISTS retrieval_feedback (
    id              UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    workspace_id    UUID        NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    memory_id       UUID        NOT NULL REFERENCES memory_units(id) ON DELETE CASCADE,
    query_id        TEXT        NOT NULL,
    agent_id        TEXT,
    user_id         TEXT,
    rating          SMALLINT    NOT NULL CHECK (rating IN (-1, 0, 1)),
    comment         TEXT,
    occurred_at     TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_retrieval_feedback_memory
    ON retrieval_feedback (memory_id, occurred_at DESC);

CREATE INDEX IF NOT EXISTS idx_retrieval_feedback_workspace
    ON retrieval_feedback (workspace_id, occurred_at DESC);

ALTER TABLE memory_units
    ADD COLUMN IF NOT EXISTS relevance_score DOUBLE PRECISION
        NOT NULL DEFAULT 0.5;

CREATE INDEX IF NOT EXISTS idx_memory_units_relevance
    ON memory_units (workspace_id, relevance_score DESC)
    WHERE deleted_at IS NULL;