-- Skill governance: scope visibility, rate limit + circuit breaker config,
-- invocation log, and new audit actions for skill lifecycle events.

-- 1) Per-skill scope visibility (private = creator/workspace key only,
--    workspace = visible to all keys in the workspace). Published-across-
--    workspaces semantics are intentionally out of scope here (a workspace key
--    is already workspace-bound).
ALTER TABLE workspace_skills
    ADD COLUMN IF NOT EXISTS scope_visibility TEXT NOT NULL DEFAULT 'workspace'
        CHECK (scope_visibility IN ('private', 'workspace'));

-- 2) Per-skill rate limit + circuit breaker config.
--    rate_limit_per_minute = 0 disables rate limiting for this skill.
--    circuit_breaker_threshold = 0 disables circuit breaker for this skill.
ALTER TABLE workspace_skills
    ADD COLUMN IF NOT EXISTS rate_limit_per_minute INT NOT NULL DEFAULT 0
        CHECK (rate_limit_per_minute >= 0),
    ADD COLUMN IF NOT EXISTS circuit_breaker_threshold INT NOT NULL DEFAULT 0
        CHECK (circuit_breaker_threshold >= 0),
    ADD COLUMN IF NOT EXISTS circuit_breaker_cooldown_seconds INT NOT NULL DEFAULT 60
        CHECK (circuit_breaker_cooldown_seconds >= 1);

-- Also propagate scope_visibility into version snapshots so historical reads
-- reflect the visibility at write time. Backfilled to current value.
ALTER TABLE workspace_skill_versions
    ADD COLUMN IF NOT EXISTS scope_visibility TEXT NOT NULL DEFAULT 'workspace'
        CHECK (scope_visibility IN ('private', 'workspace'));

-- 3) Invocation log: records every call made to a skill (via HTTP /invoke or
--    MCP skill_invoke). Powers the circuit breaker, rate limit window, and
--    operator observability of "which version was actually called".
CREATE TABLE IF NOT EXISTS workspace_skill_invocations (
    id BIGSERIAL PRIMARY KEY,
    skill_id UUID NOT NULL REFERENCES workspace_skills(id) ON DELETE CASCADE,
    workspace_id UUID NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    skill_name TEXT NOT NULL,
    skill_version INT NOT NULL,
    actor TEXT NOT NULL,
    source TEXT NOT NULL CHECK (source IN ('http', 'mcp', 'test')),
    status_code INT NOT NULL,
    latency_ms INT NOT NULL CHECK (latency_ms >= 0),
    error TEXT,
    occurred_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS workspace_skill_invocations_skill_id_time_idx
    ON workspace_skill_invocations (skill_id, occurred_at DESC);
CREATE INDEX IF NOT EXISTS workspace_skill_invocations_workspace_time_idx
    ON workspace_skill_invocations (workspace_id, occurred_at DESC);

-- 4) Audit actions for skill lifecycle events.
ALTER TYPE audit_action ADD VALUE IF NOT EXISTS 'skill_created';
ALTER TYPE audit_action ADD VALUE IF NOT EXISTS 'skill_updated';
ALTER TYPE audit_action ADD VALUE IF NOT EXISTS 'skill_deleted';
ALTER TYPE audit_action ADD VALUE IF NOT EXISTS 'skill_rolled_back';
ALTER TYPE audit_action ADD VALUE IF NOT EXISTS 'skill_invoked';
