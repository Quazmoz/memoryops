CREATE TABLE IF NOT EXISTS retrieval_traces (
    id UUID PRIMARY KEY,
    workspace_id UUID NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    query_id UUID NOT NULL,
    trace JSONB NOT NULL,
    retrieved_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    expires_at TIMESTAMPTZ NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

COMMENT ON COLUMN retrieval_traces.expires_at IS 'Retrieval traces are retained for 30 days by default and filtered by expires_at.';

CREATE INDEX IF NOT EXISTS idx_retrieval_traces_workspace_query
    ON retrieval_traces(workspace_id, query_id);

CREATE INDEX IF NOT EXISTS idx_retrieval_traces_expires
    ON retrieval_traces(expires_at);