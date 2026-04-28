import crypto from 'node:crypto';

const API_BASE = process.env.E2E_API_BASE ?? 'http://localhost:5173';

export async function seedGitHubEvent(workspaceId: string, apiKey: string): Promise<void> {
  const now = new Date().toISOString();
  const line = JSON.stringify({
    id: crypto.randomUUID(),
    workspace_id: workspaceId,
    scope: {
      workspace_id: workspaceId,
      agent_id: null,
      user_id: null,
      repo: 'e2e/test-repo',
    },
    memory_type: 'episodic',
    content: 'GitHub push event on refs/heads/main: E2E seed commit for push event',
    entities: [],
    importance_score: 0.8,
    importance_overridden: false,
    source_events: [],
    embedding_id: null,
    token_count: 10,
    decay_score: 1.0,
    pinned: false,
    tags: ['push', 'e2e'],
    version: 1,
    promoted_at: null,
    source_episode_ids: [],
    corroboration_count: 1,
    deleted_at: null,
    last_accessed_at: null,
    created_at: now,
    updated_at: now,
  });

  const res = await fetch(`${API_BASE}/v1/workspaces/${workspaceId}/import`, {
    method: 'POST',
    headers: {
      'Content-Type': 'application/x-ndjson',
      'X-API-Key': apiKey,
    },
    body: `${line}\n`,
  });

  if (!res.ok) {
    const detail = await res.text();
    throw new Error(`seedGitHubEvent failed: ${res.status} — ${detail}`);
  }
}

// Polls GET /v1/memory until count > 0 or timeout
export async function waitForMemory(
  workspaceId: string,
  apiKey: string,
  timeoutMs = 90_000,
): Promise<void> {
  const start = Date.now();
  const pollInterval = 1_000;
  const fetchTimeoutMs = 20_000;

  while (Date.now() - start < timeoutMs) {
    const controller = new AbortController();
    const abortId = setTimeout(() => controller.abort(), fetchTimeoutMs);
    try {
      const res = await fetch(
        `${API_BASE}/v1/memory?workspace_id=${workspaceId}&limit=1`,
        {
          signal: controller.signal,
          headers: {
            'X-API-Key': apiKey,
            'X-Workspace-Id': workspaceId,
          },
        },
      );

      if (res.ok) {
        const data = (await res.json()) as { items?: unknown[]; memories?: unknown[]; total?: number };
        const count = data.items?.length ?? data.memories?.length ?? data.total ?? 0;
        if (count > 0) {
          return;
        }
      }
    } catch {
      // Network error or abort timeout — retry
    } finally {
      clearTimeout(abortId);
    }

    await new Promise((resolve) => setTimeout(resolve, pollInterval));
  }

  throw new Error(`waitForMemory timed out after ${timeoutMs}ms`);
}
