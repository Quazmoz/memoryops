-- Workspace purge jobs track background deletion of workspace-associated data
-- (memories, embeddings, audit logs, etc.) after a workspace is soft-deleted.

CREATE TABLE IF NOT EXISTS workspace_purge_jobs (
    id            UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    workspace_id  UUID        NOT NULL,
    status        TEXT        NOT NULL DEFAULT 'pending'
                                  CHECK (status IN ('pending', 'running', 'done', 'failed')),
    started_at    TIMESTAMPTZ,
    finished_at   TIMESTAMPTZ,
    error         TEXT,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at    TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_workspace_purge_jobs_workspace_id
    ON workspace_purge_jobs (workspace_id);

CREATE INDEX IF NOT EXISTS idx_workspace_purge_jobs_status
    ON workspace_purge_jobs (status)
    WHERE status IN ('pending', 'running');
