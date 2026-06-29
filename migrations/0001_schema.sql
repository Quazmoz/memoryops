-- ============================================================================
-- 0001_schema.sql — Core schema: enums, functions, tables, triggers, indexes
-- ============================================================================
-- Consolidated from migrations 0001–0036 (see _archived/ for originals).

-- ── Enum types ───────────────────────────────────────────────────────────────

CREATE TYPE source AS ENUM (
    'github',
    'slack',
    'jira',
    'linear',
    'observation'
);

CREATE TYPE event_type AS ENUM (
    'pull_request',
    'pull_request_review',
    'push',
    'issue_comment',
    'issue',
    'message',
    'reaction',
    'agent_observation'
);

CREATE TYPE memory_type AS ENUM ('episodic', 'semantic');

CREATE TYPE audit_action AS ENUM (
    -- Memory lifecycle
    'memory_created',
    'memory_edited',
    'memory_deleted',
    'memory_restored',
    'memory_pinned',
    'memory_unpinned',
    'memory_promoted',
    'memory_merged',
    'memory_embedded',
    'memory_hard_deleted',
    'memory_imported',
    'memory_exported',
    'importance_overridden',
    'observation_ingested',

    -- Workspace lifecycle
    'workspace_created',
    'workspace_bootstrap',
    'workspace_deleted',
    'workspace_config_updated',
    'workspace.promote',
    'workspace_reindexed',
    'config_updated',
    'publish',

    -- API keys
    'key_created',
    'key_revoked',
    'auth_failed',

    -- Integrations
    'integration_added',
    'integration_removed',
    'integration_updated',
    'integration_webhook_secret_changed',

    -- Contradictions
    'contradiction_resolved',
    'contradiction_dismissed',

    -- Compliance & audit
    'user_erasure',
    'audit_exported',

    -- Retrieval
    'retrieval_feedback',

    -- Skills (legacy names, kept for backward compat with existing rows)
    'skill_created',
    'skill_updated',
    'skill_deleted',
    'skill_rolled_back',
    'skill_invoked',

    -- Tools
    'tool_created',
    'tool_updated',
    'tool_deleted',
    'tool_rolled_back',
    'tool_invoked',
    'tool_secret_revealed',

    -- Agent resources
    'agent_resource_created',
    'agent_resource_updated',
    'agent_resource_deleted',
    'agent_resource_rolled_back'
);

CREATE TYPE integration_status AS ENUM ('active', 'degraded', 'failing');

-- ── Utility functions ────────────────────────────────────────────────────────

CREATE OR REPLACE FUNCTION set_updated_at()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = now();
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

-- ── Tables ───────────────────────────────────────────────────────────────────

CREATE TABLE workspaces (
    id                      UUID        PRIMARY KEY,
    name                    TEXT        NOT NULL UNIQUE,
    config                  JSONB       NOT NULL DEFAULT '{}'::jsonb,
    promotion_threshold     FLOAT8      NOT NULL DEFAULT 0.72,
    dedup_cosine_threshold  FLOAT8      NOT NULL DEFAULT 0.92,
    created_from_ip         INET,
    created_at              TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at              TIMESTAMPTZ NOT NULL DEFAULT now(),
    deleted_at              TIMESTAMPTZ
);

CREATE TABLE api_keys (
    id              UUID        PRIMARY KEY,
    workspace_id    UUID        NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    name            TEXT        NOT NULL,
    key_hash        TEXT        NOT NULL,
    prefix          VARCHAR(32) NOT NULL,
    prefix_version  SMALLINT    NOT NULL DEFAULT 1,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    last_used_at    TIMESTAMPTZ,
    revoked         BOOLEAN     NOT NULL DEFAULT false,
    revoked_at      TIMESTAMPTZ,
    CONSTRAINT api_keys_prefix_len CHECK (char_length(prefix) IN (8, 13, 21)),
    CONSTRAINT api_keys_prefix_version_supported CHECK (prefix_version IN (1, 2, 3)),
    CONSTRAINT api_keys_revoked_at_consistent CHECK (
        (revoked = false AND revoked_at IS NULL)
        OR (revoked = true AND revoked_at IS NOT NULL)
    )
);

COMMENT ON COLUMN api_keys.prefix_version IS
    '1 = legacy 8-character prefix requiring rotation; 2 = 13-character workspace prefix; 3 = 21-character prefix with random entropy';

CREATE TABLE raw_events (
    id              UUID        PRIMARY KEY,
    workspace_id    UUID        NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    source          source      NOT NULL,
    event_type      event_type  NOT NULL,
    actor           TEXT        NOT NULL,
    payload         JSONB       NOT NULL,
    idempotency_key TEXT        NOT NULL,
    occurred_at     TIMESTAMPTZ NOT NULL,
    ingested_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    slack_channel   TEXT,
    slack_thread_ts TEXT
);

CREATE TABLE memory_units (
    id                      UUID            PRIMARY KEY,
    workspace_id            UUID            NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    scope                   JSONB           NOT NULL,
    memory_type             memory_type     NOT NULL DEFAULT 'episodic',
    content                 TEXT            NOT NULL,
    entities                JSONB           NOT NULL DEFAULT '[]'::jsonb,
    importance_score        REAL            NOT NULL,
    importance_overridden   BOOLEAN         NOT NULL DEFAULT false,
    source_events           UUID[]          NOT NULL DEFAULT ARRAY[]::UUID[],
    embedding_id            TEXT,
    token_count             INTEGER,
    decay_score             REAL            NOT NULL DEFAULT 1.0,
    pinned                  BOOLEAN         NOT NULL DEFAULT false,
    tags                    TEXT[]          NOT NULL DEFAULT ARRAY[]::TEXT[],
    version                 INTEGER         NOT NULL DEFAULT 1,
    access_count            INTEGER         NOT NULL DEFAULT 0,
    hard_deleted_at         TIMESTAMPTZ,
    promoted_at             TIMESTAMPTZ,
    source_episode_ids      UUID[]          DEFAULT '{}',
    corroboration_count     INTEGER         NOT NULL DEFAULT 1,
    scope_visibility        VARCHAR(16)     NOT NULL DEFAULT 'private',
    relevance_score         DOUBLE PRECISION NOT NULL DEFAULT 0.5,
    deleted_at              TIMESTAMPTZ,
    last_accessed_at        TIMESTAMPTZ,
    created_at              TIMESTAMPTZ     NOT NULL DEFAULT now(),
    updated_at              TIMESTAMPTZ     NOT NULL DEFAULT now(),
    -- Generated columns for fast scope-filtered queries
    agent_id                TEXT            GENERATED ALWAYS AS (scope->>'agent_id') STORED,
    user_id                 TEXT            GENERATED ALWAYS AS (scope->>'user_id') STORED,
    repo                    TEXT            GENERATED ALWAYS AS (scope->>'repo') STORED,
    CONSTRAINT memory_units_importance_range CHECK (importance_score >= 0.0 AND importance_score <= 1.0),
    CONSTRAINT memory_units_decay_range CHECK (decay_score >= 0.0 AND decay_score <= 1.0),
    CONSTRAINT memory_units_version_positive CHECK (version > 0),
    CONSTRAINT memory_units_token_count_non_negative CHECK (token_count IS NULL OR token_count >= 0),
    CONSTRAINT memory_units_scope_visibility_check CHECK (scope_visibility IN ('private', 'workspace'))
);

CREATE TABLE memory_versions (
    id                UUID        PRIMARY KEY,
    memory_id         UUID        NOT NULL REFERENCES memory_units(id) ON DELETE CASCADE,
    workspace_id      UUID        NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    version           INTEGER     NOT NULL,
    content           TEXT        NOT NULL,
    importance_score  REAL        NOT NULL,
    tags              TEXT[]      NOT NULL DEFAULT ARRAY[]::TEXT[],
    edited_by         TEXT        NOT NULL,
    created_at        TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at        TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT memory_versions_importance_range CHECK (importance_score >= 0.0 AND importance_score <= 1.0),
    CONSTRAINT memory_versions_version_positive CHECK (version > 0),
    CONSTRAINT memory_versions_unique_version UNIQUE (memory_id, version)
);

CREATE TABLE processing_state (
    raw_event_id    UUID        PRIMARY KEY REFERENCES raw_events(id) ON DELETE CASCADE,
    workspace_id    UUID        NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    status          TEXT        NOT NULL CHECK (status IN ('processing','done','failed')),
    attempts        INTEGER     NOT NULL DEFAULT 0,
    last_error      TEXT,
    processed_at    TIMESTAMPTZ,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE integration_health (
    workspace_id    UUID                NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    source          source              NOT NULL,
    last_event_at   TIMESTAMPTZ,
    events_24h      BIGINT              NOT NULL DEFAULT 0,
    errors_24h      BIGINT              NOT NULL DEFAULT 0,
    status          integration_status  NOT NULL DEFAULT 'active',
    created_at      TIMESTAMPTZ         NOT NULL DEFAULT now(),
    updated_at      TIMESTAMPTZ         NOT NULL DEFAULT now(),
    PRIMARY KEY (workspace_id, source),
    CONSTRAINT integration_health_events_non_negative CHECK (events_24h >= 0),
    CONSTRAINT integration_health_errors_non_negative CHECK (errors_24h >= 0)
);

CREATE TABLE integrations (
    workspace_id        UUID        NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    source              source      NOT NULL,
    webhook_secret_hash TEXT,
    webhook_secret_enc  TEXT,
    api_token_enc       TEXT,
    api_sync_enabled    BOOLEAN     NOT NULL DEFAULT FALSE,
    sync_config         JSONB       NOT NULL DEFAULT '{}'::jsonb,
    last_sync_at        TIMESTAMPTZ,
    created_at          TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at          TIMESTAMPTZ NOT NULL DEFAULT now(),
    deleted_at          TIMESTAMPTZ,
    PRIMARY KEY (workspace_id, source)
);

COMMENT ON COLUMN integrations.webhook_secret_enc IS
    'AES-GCM encrypted HMAC signing secret for per-workspace webhooks. Recreate integrations that only have legacy webhook_secret_hash values.';
COMMENT ON COLUMN integrations.api_token_enc IS
    'AES-GCM encrypted platform API token used by connector sync/backfill adapters.';
COMMENT ON COLUMN integrations.api_sync_enabled IS
    'Whether API-based connector sync/backfill is enabled for this integration.';
COMMENT ON COLUMN integrations.sync_config IS
    'Connector-specific sync settings such as selected repositories, channels, projects, cursors, or resource types.';
COMMENT ON COLUMN integrations.last_sync_at IS
    'Timestamp of the most recent API connector sync attempt for this integration.';

-- ── Triggers ─────────────────────────────────────────────────────────────────

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

CREATE TRIGGER trg_integration_health_updated_at
    BEFORE UPDATE ON integration_health
    FOR EACH ROW EXECUTE FUNCTION set_updated_at();

CREATE TRIGGER trg_integrations_updated_at
    BEFORE UPDATE ON integrations
    FOR EACH ROW EXECUTE FUNCTION set_updated_at();

CREATE TRIGGER trg_processing_state_updated_at
    BEFORE UPDATE ON processing_state
    FOR EACH ROW EXECUTE FUNCTION set_updated_at();

-- ── Indexes: workspaces ──────────────────────────────────────────────────────

CREATE INDEX idx_workspaces_active
    ON workspaces(id)
    WHERE deleted_at IS NULL;

-- ── Indexes: api_keys ────────────────────────────────────────────────────────

CREATE INDEX idx_api_keys_workspace_prefix
    ON api_keys(workspace_id, prefix)
    WHERE revoked = false;

CREATE INDEX idx_api_keys_prefix
    ON api_keys(prefix);

-- ── Indexes: raw_events ──────────────────────────────────────────────────────

CREATE INDEX idx_raw_events_workspace_occurred
    ON raw_events(workspace_id, occurred_at DESC);

CREATE UNIQUE INDEX idx_raw_events_workspace_idempotency
    ON raw_events(workspace_id, idempotency_key);

CREATE INDEX idx_raw_events_workspace_source_type
    ON raw_events(workspace_id, source, event_type, occurred_at DESC);

CREATE INDEX idx_raw_events_slack_channel
    ON raw_events(workspace_id, slack_channel)
    WHERE source = 'slack' AND slack_channel IS NOT NULL;

CREATE INDEX idx_raw_events_linear_jira_workspace_type
    ON raw_events(workspace_id, source, event_type, occurred_at DESC)
    WHERE source IN ('linear', 'jira');

CREATE INDEX idx_raw_events_observation
    ON raw_events(workspace_id, ingested_at DESC)
    WHERE source = 'observation';

CREATE INDEX idx_raw_events_source_ref_path
    ON raw_events(workspace_id, (split_part(payload->>'source_ref', '#', 1)))
    WHERE source = 'observation';

-- ── Indexes: memory_units ────────────────────────────────────────────────────

CREATE INDEX idx_memory_units_workspace_type
    ON memory_units(workspace_id, memory_type);

CREATE INDEX idx_memory_units_decay
    ON memory_units(workspace_id, decay_score)
    WHERE deleted_at IS NULL;

CREATE INDEX idx_memory_units_scope
    ON memory_units(
        workspace_id,
        (scope->>'agent_id'),
        (scope->>'user_id'),
        (scope->>'repo')
    );

CREATE INDEX idx_memory_units_tags
    ON memory_units USING gin(tags);

CREATE INDEX idx_memory_units_fts
    ON memory_units USING GIN (to_tsvector('english', content));

CREATE INDEX idx_memory_units_workspace_type_score
    ON memory_units(workspace_id, memory_type, importance_score DESC)
    WHERE deleted_at IS NULL;

CREATE INDEX idx_memory_units_workspace_pinned
    ON memory_units(workspace_id, pinned, updated_at DESC)
    WHERE deleted_at IS NULL AND pinned = true;

CREATE INDEX idx_memory_units_workspace_active_updated
    ON memory_units(workspace_id, updated_at DESC)
    WHERE deleted_at IS NULL;

CREATE INDEX idx_memory_units_workspace_deleted
    ON memory_units(workspace_id, deleted_at DESC)
    WHERE deleted_at IS NOT NULL;

CREATE INDEX idx_memory_units_pruning
    ON memory_units(decay_score)
    WHERE deleted_at IS NULL
      AND pinned = false
      AND importance_overridden = false;

CREATE INDEX idx_memory_units_hard_delete
    ON memory_units(deleted_at)
    WHERE deleted_at IS NOT NULL;

CREATE INDEX idx_memory_units_promotion_candidates
    ON memory_units(workspace_id, memory_type, decay_score)
    WHERE memory_type = 'episodic'
      AND deleted_at IS NULL
      AND embedding_id IS NOT NULL;

CREATE INDEX idx_memory_units_semantic
    ON memory_units(workspace_id, memory_type)
    WHERE memory_type = 'semantic' AND deleted_at IS NULL;

CREATE INDEX idx_memory_units_scope_filter
    ON memory_units(workspace_id, agent_id, user_id, repo)
    WHERE deleted_at IS NULL;

CREATE INDEX idx_memory_units_source_events
    ON memory_units USING GIN(source_events);

CREATE INDEX idx_memory_units_source_episode_ids
    ON memory_units USING GIN(source_episode_ids);

CREATE INDEX idx_memory_units_created_at
    ON memory_units(workspace_id, created_at)
    WHERE deleted_at IS NULL;

CREATE INDEX idx_memory_units_scope_visibility
    ON memory_units(workspace_id, scope_visibility)
    WHERE deleted_at IS NULL AND memory_type = 'semantic';

CREATE INDEX idx_memory_units_relevance
    ON memory_units(workspace_id, relevance_score DESC)
    WHERE deleted_at IS NULL;

CREATE INDEX idx_memory_units_observation_agent
    ON memory_units((scope->>'agent_id'), created_at DESC)
    WHERE scope->>'source' = 'observation';

-- ── Indexes: memory_versions ─────────────────────────────────────────────────

CREATE INDEX idx_memory_versions_memory_as_of
    ON memory_versions(memory_id, created_at DESC);

-- ── Indexes: processing_state ────────────────────────────────────────────────

CREATE INDEX idx_processing_state_workspace_status
    ON processing_state(workspace_id, status);

-- ── Indexes: integrations ────────────────────────────────────────────────────

CREATE INDEX idx_integrations_active
    ON integrations(workspace_id, source)
    WHERE deleted_at IS NULL;

CREATE INDEX idx_integrations_active_source
    ON integrations(source, workspace_id)
    WHERE deleted_at IS NULL;
