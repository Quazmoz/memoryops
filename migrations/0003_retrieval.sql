-- ============================================================================
-- 0003_retrieval.sql — Retrieval traces and feedback
-- ============================================================================
-- Consolidated from migrations 0009, 0021.

CREATE TABLE retrieval_traces (
    id              UUID        PRIMARY KEY,
    workspace_id    UUID        NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    query_id        UUID        NOT NULL,
    trace           JSONB       NOT NULL,
    retrieved_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    expires_at      TIMESTAMPTZ NOT NULL,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);

COMMENT ON COLUMN retrieval_traces.expires_at IS
    'Retrieval traces are retained for 30 days by default and filtered by expires_at.';

CREATE TRIGGER trg_retrieval_traces_updated_at
    BEFORE UPDATE ON retrieval_traces
    FOR EACH ROW EXECUTE FUNCTION set_updated_at();

CREATE TABLE retrieval_feedback (
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

-- ── Indexes: retrieval_traces ────────────────────────────────────────────────

CREATE INDEX idx_retrieval_traces_workspace_query
    ON retrieval_traces(workspace_id, query_id);

CREATE INDEX idx_retrieval_traces_expires
    ON retrieval_traces(expires_at);

-- ── Indexes: retrieval_feedback ──────────────────────────────────────────────

CREATE INDEX idx_retrieval_feedback_memory
    ON retrieval_feedback(memory_id, occurred_at DESC);

CREATE INDEX idx_retrieval_feedback_workspace
    ON retrieval_feedback(workspace_id, occurred_at DESC);
