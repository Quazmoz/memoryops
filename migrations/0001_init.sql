CREATE TYPE source AS ENUM ('github', 'slack', 'jira', 'linear');

CREATE TYPE event_type AS ENUM (
    'pull_request',
    'pull_request_review',
    'push',
    'issue_comment',
    'issue',
    'message',
    'reaction'
);

CREATE TYPE memory_type AS ENUM ('episodic', 'semantic');

CREATE TYPE audit_action AS ENUM (
    'memory_created',
    'memory_edited',
    'memory_deleted',
    'memory_restored',
    'memory_pinned',
    'memory_unpinned',
    'memory_promoted',
    'memory_merged',
    'importance_overridden',
    'key_created',
    'key_revoked',
    'workspace_config_updated',
    'integration_added',
    'integration_removed'
);

CREATE TYPE integration_status AS ENUM ('active', 'degraded', 'failing');

CREATE OR REPLACE FUNCTION set_updated_at()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = now();
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TABLE workspaces (
    id UUID PRIMARY KEY,
    name TEXT NOT NULL UNIQUE,
    config JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    deleted_at TIMESTAMPTZ
);

CREATE TABLE api_keys (
    id UUID PRIMARY KEY,
    workspace_id UUID NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    key_hash TEXT NOT NULL,
    prefix TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    last_used_at TIMESTAMPTZ,
    revoked BOOLEAN NOT NULL DEFAULT false,
    revoked_at TIMESTAMPTZ,
    CONSTRAINT api_keys_prefix_len CHECK (char_length(prefix) = 8),
    CONSTRAINT api_keys_revoked_at_consistent CHECK (
        (revoked = false AND revoked_at IS NULL)
        OR (revoked = true AND revoked_at IS NOT NULL)
    )
);

CREATE TABLE raw_events (
    id UUID PRIMARY KEY,
    workspace_id UUID NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    source source NOT NULL,
    event_type event_type NOT NULL,
    actor TEXT NOT NULL,
    payload JSONB NOT NULL,
    idempotency_key TEXT NOT NULL,
    occurred_at TIMESTAMPTZ NOT NULL,
    ingested_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE memory_units (
    id UUID PRIMARY KEY,
    workspace_id UUID NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    scope JSONB NOT NULL,
    memory_type memory_type NOT NULL,
    content TEXT NOT NULL,
    entities JSONB NOT NULL DEFAULT '[]'::jsonb,
    importance_score REAL NOT NULL,
    importance_overridden BOOLEAN NOT NULL DEFAULT false,
    source_events UUID[] NOT NULL DEFAULT ARRAY[]::UUID[],
    embedding_id TEXT,
    token_count INTEGER,
    decay_score REAL NOT NULL DEFAULT 1.0,
    pinned BOOLEAN NOT NULL DEFAULT false,
    tags TEXT[] NOT NULL DEFAULT ARRAY[]::TEXT[],
    version INTEGER NOT NULL DEFAULT 1,
    deleted_at TIMESTAMPTZ,
    last_accessed_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT memory_units_importance_range CHECK (importance_score >= 0.0 AND importance_score <= 1.0),
    CONSTRAINT memory_units_decay_range CHECK (decay_score >= 0.0 AND decay_score <= 1.0),
    CONSTRAINT memory_units_version_positive CHECK (version > 0),
    CONSTRAINT memory_units_token_count_non_negative CHECK (token_count IS NULL OR token_count >= 0)
);

CREATE TABLE memory_versions (
    id UUID PRIMARY KEY,
    memory_id UUID NOT NULL REFERENCES memory_units(id) ON DELETE CASCADE,
    workspace_id UUID NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    version INTEGER NOT NULL,
    content TEXT NOT NULL,
    importance_score REAL NOT NULL,
    tags TEXT[] NOT NULL DEFAULT ARRAY[]::TEXT[],
    edited_by TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT memory_versions_importance_range CHECK (importance_score >= 0.0 AND importance_score <= 1.0),
    CONSTRAINT memory_versions_version_positive CHECK (version > 0),
    CONSTRAINT memory_versions_unique_version UNIQUE (memory_id, version)
);

CREATE TABLE audit_log (
    id UUID PRIMARY KEY,
    workspace_id UUID NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    actor TEXT NOT NULL,
    action audit_action NOT NULL,
    target_id UUID NOT NULL,
    target_type TEXT NOT NULL,
    diff JSONB,
    occurred_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE integration_health (
    workspace_id UUID NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    source source NOT NULL,
    last_event_at TIMESTAMPTZ,
    events_24h BIGINT NOT NULL DEFAULT 0,
    errors_24h BIGINT NOT NULL DEFAULT 0,
    status integration_status NOT NULL DEFAULT 'active',
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (workspace_id, source),
    CONSTRAINT integration_health_events_non_negative CHECK (events_24h >= 0),
    CONSTRAINT integration_health_errors_non_negative CHECK (errors_24h >= 0)
);

CREATE TABLE retrieval_traces (
    id UUID PRIMARY KEY,
    workspace_id UUID NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    query_id UUID NOT NULL,
    trace JSONB NOT NULL,
    retrieved_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    expires_at TIMESTAMPTZ NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TRIGGER trg_workspaces_updated_at
    BEFORE UPDATE ON workspaces
    FOR EACH ROW EXECUTE FUNCTION set_updated_at();

CREATE TRIGGER trg_api_keys_updated_at
    BEFORE UPDATE ON api_keys
    FOR EACH ROW EXECUTE FUNCTION set_updated_at();

CREATE TRIGGER trg_raw_events_updated_at
    BEFORE UPDATE ON raw_events
    FOR EACH ROW EXECUTE FUNCTION set_updated_at();

CREATE TRIGGER trg_memory_units_updated_at
    BEFORE UPDATE ON memory_units
    FOR EACH ROW EXECUTE FUNCTION set_updated_at();

CREATE TRIGGER trg_memory_versions_updated_at
    BEFORE UPDATE ON memory_versions
    FOR EACH ROW EXECUTE FUNCTION set_updated_at();

CREATE TRIGGER trg_audit_log_updated_at
    BEFORE UPDATE ON audit_log
    FOR EACH ROW EXECUTE FUNCTION set_updated_at();

CREATE TRIGGER trg_integration_health_updated_at
    BEFORE UPDATE ON integration_health
    FOR EACH ROW EXECUTE FUNCTION set_updated_at();

CREATE TRIGGER trg_retrieval_traces_updated_at
    BEFORE UPDATE ON retrieval_traces
    FOR EACH ROW EXECUTE FUNCTION set_updated_at();

CREATE INDEX idx_raw_events_workspace_occurred ON raw_events(workspace_id, occurred_at DESC);
CREATE UNIQUE INDEX idx_raw_events_idempotency ON raw_events(idempotency_key);

CREATE INDEX idx_memory_units_workspace_type ON memory_units(workspace_id, memory_type);
CREATE INDEX idx_memory_units_decay ON memory_units(workspace_id, decay_score) WHERE deleted_at IS NULL;
CREATE INDEX idx_memory_units_scope ON memory_units(
    workspace_id,
    (scope->>'agent_id'),
    (scope->>'user_id'),
    (scope->>'repo')
);
CREATE INDEX idx_memory_units_tags ON memory_units USING gin(tags);

CREATE INDEX idx_audit_log_workspace_time ON audit_log(workspace_id, occurred_at DESC);
CREATE INDEX idx_api_keys_workspace_prefix ON api_keys(workspace_id, prefix) WHERE revoked = false;
CREATE INDEX idx_retrieval_traces_workspace_query ON retrieval_traces(workspace_id, query_id);
CREATE INDEX idx_retrieval_traces_expires ON retrieval_traces(expires_at);
