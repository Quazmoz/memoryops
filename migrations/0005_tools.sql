-- ============================================================================
-- 0005_tools.sql — Workspace tools (versioning, governance, invocations)
-- ============================================================================
-- Consolidated from migrations 0016, 0037, 0038, 0039, 0040.

CREATE TABLE workspace_tools (
    id                              UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    workspace_id                    UUID        NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    name                            TEXT        NOT NULL,
    description                     TEXT        NOT NULL,
    endpoint_url                    TEXT        NOT NULL,
    http_method                     TEXT        NOT NULL DEFAULT 'POST',
    input_schema                    JSONB       NOT NULL DEFAULT '{}',
    output_schema                   JSONB       NOT NULL DEFAULT '{}',
    auth_header                     TEXT,
    auth_secret_enc                 TEXT,
    enabled                         BOOL        NOT NULL DEFAULT TRUE,
    version                         INT         NOT NULL DEFAULT 1,
    scope_visibility                TEXT        NOT NULL DEFAULT 'workspace',
    rate_limit_per_minute           INT         NOT NULL DEFAULT 0,
    circuit_breaker_threshold       INT         NOT NULL DEFAULT 0,
    circuit_breaker_cooldown_seconds INT        NOT NULL DEFAULT 60,
    created_at                      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at                      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(workspace_id, name),
    CHECK (http_method IN ('GET', 'POST', 'PUT')),
    CONSTRAINT workspace_tools_scope_visibility_check
        CHECK (scope_visibility IN ('private', 'workspace', 'published')),
    CHECK (rate_limit_per_minute >= 0),
    CHECK (circuit_breaker_threshold >= 0),
    CHECK (circuit_breaker_cooldown_seconds >= 1)
);

CREATE TABLE workspace_tool_versions (
    id               UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    tool_id          UUID        NOT NULL REFERENCES workspace_tools(id) ON DELETE CASCADE,
    workspace_id     UUID        NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    name             TEXT        NOT NULL,
    version          INT         NOT NULL,
    description      TEXT        NOT NULL,
    endpoint_url     TEXT        NOT NULL,
    http_method      TEXT        NOT NULL,
    input_schema     JSONB       NOT NULL,
    output_schema    JSONB       NOT NULL,
    auth_header      TEXT,
    auth_secret_enc  TEXT,
    enabled          BOOL        NOT NULL,
    scope_visibility TEXT        NOT NULL DEFAULT 'workspace',
    change_note      TEXT,
    created_by       TEXT,
    created_at       TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(tool_id, version),
    CHECK (version >= 1),
    CHECK (http_method IN ('GET', 'POST', 'PUT')),
    CONSTRAINT workspace_tool_versions_scope_visibility_check
        CHECK (scope_visibility IN ('private', 'workspace', 'published'))
);

CREATE TABLE workspace_tool_invocations (
    id              BIGSERIAL   PRIMARY KEY,
    tool_id         UUID        NOT NULL REFERENCES workspace_tools(id) ON DELETE CASCADE,
    workspace_id    UUID        NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    tool_name       TEXT        NOT NULL,
    tool_version    INT         NOT NULL,
    actor           TEXT        NOT NULL,
    source          TEXT        NOT NULL CHECK (source IN ('http', 'mcp', 'test')),
    status_code     INT         NOT NULL,
    latency_ms      INT         NOT NULL CHECK (latency_ms >= 0),
    error           TEXT,
    occurred_at     TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- ── Triggers ─────────────────────────────────────────────────────────────────

CREATE TRIGGER trg_workspace_tools_updated_at
    BEFORE UPDATE ON workspace_tools
    FOR EACH ROW EXECUTE FUNCTION set_updated_at();

-- ── Indexes ──────────────────────────────────────────────────────────────────

CREATE INDEX workspace_tools_workspace_id_enabled
    ON workspace_tools(workspace_id) WHERE enabled = TRUE;

CREATE INDEX workspace_tool_versions_workspace_name_version_idx
    ON workspace_tool_versions(workspace_id, name, version DESC);

CREATE INDEX workspace_tool_invocations_tool_id_time_idx
    ON workspace_tool_invocations(tool_id, occurred_at DESC);

CREATE INDEX workspace_tool_invocations_workspace_time_idx
    ON workspace_tool_invocations(workspace_id, occurred_at DESC);
