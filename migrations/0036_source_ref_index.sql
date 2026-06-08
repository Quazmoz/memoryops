-- Supports the `source_ref` filter on GET /v1/memory (memories referencing a
-- specific source file, e.g. the VS Code "memories referencing this file"
-- CodeLens). The list query collects matching raw events with
--   split_part(payload->>'source_ref', '#', 1) = <path>
-- then overlaps them against memory_units.source_events (GIN index from 0018).
--
-- This functional index makes the raw-events subquery a fast index scan instead
-- of a sequential scan over the workspace's events. Limited to observation
-- events, which are the only source that carries a file-level source_ref.
CREATE INDEX IF NOT EXISTS idx_raw_events_source_ref_path
    ON raw_events (workspace_id, (split_part(payload->>'source_ref', '#', 1)))
    WHERE source = 'observation';
