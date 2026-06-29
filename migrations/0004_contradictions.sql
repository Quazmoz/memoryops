-- ============================================================================
-- 0004_contradictions.sql — Contradiction detection and resolution
-- ============================================================================
-- Consolidated from migrations 0017, 0022.

CREATE TYPE contradiction_resolution AS ENUM (
    'open',
    'auto_resolved',
    'dismissed',
    'accepted',
    'keep_a',
    'keep_b'
);

CREATE TABLE contradiction_flags (
    id                  UUID                    PRIMARY KEY DEFAULT gen_random_uuid(),
    workspace_id        UUID                    NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    memory_id_a         UUID                    NOT NULL REFERENCES memory_units(id) ON DELETE CASCADE,
    memory_id_b         UUID                    NOT NULL REFERENCES memory_units(id) ON DELETE CASCADE,
    similarity          FLOAT                   NOT NULL,
    conflict_score      FLOAT                   NOT NULL,
    resolution          contradiction_resolution NOT NULL DEFAULT 'open',
    resolved_by         TEXT,
    resolved_at         TIMESTAMPTZ,
    notes               TEXT,
    kept_memory_id      UUID                    REFERENCES memory_units(id),
    discarded_memory_id UUID                    REFERENCES memory_units(id),
    created_at          TIMESTAMPTZ             NOT NULL DEFAULT NOW(),
    updated_at          TIMESTAMPTZ             NOT NULL DEFAULT NOW(),
    CHECK (memory_id_a <> memory_id_b)
);

CREATE TRIGGER trg_contradiction_flags_updated_at
    BEFORE UPDATE ON contradiction_flags
    FOR EACH ROW EXECUTE FUNCTION set_updated_at();

-- ── Indexes ──────────────────────────────────────────────────────────────────

CREATE INDEX contradiction_flags_workspace_open
    ON contradiction_flags(workspace_id, created_at DESC)
    WHERE resolution = 'open';

CREATE INDEX contradiction_flags_memory_a ON contradiction_flags(memory_id_a);
CREATE INDEX contradiction_flags_memory_b ON contradiction_flags(memory_id_b);
