CREATE TABLE processing_state (
    raw_event_id UUID PRIMARY KEY REFERENCES raw_events(id) ON DELETE CASCADE,
    workspace_id UUID NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    status TEXT NOT NULL CHECK (status IN ('processing','done','failed')),
    attempts INTEGER NOT NULL DEFAULT 0,
    last_error TEXT,
    processed_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TRIGGER trg_processing_state_updated_at
    BEFORE UPDATE ON processing_state
    FOR EACH ROW EXECUTE FUNCTION set_updated_at();

CREATE INDEX idx_processing_state_workspace_status
    ON processing_state(workspace_id, status);