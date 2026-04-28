// Seeds a memory directly via POST /v1/ingest/github with a minimal push payload
// so the test doesn't depend on the slow-path worker completing

import crypto from 'node:crypto';

const API_BASE = process.env.E2E_API_BASE ?? 'http://localhost:5173';

export async function seedGitHubEvent(workspaceId: string, apiKey: string): Promise<void> {
  const payload = {
    ref: 'refs/heads/main',
    before: '0000000000000000000000000000000000000000',
    after: 'abc123def456abc123def456abc123def456abc1',
    pusher: { name: 'e2e-bot', email: 'e2e@test.local' },
    repository: { full_name: 'e2e/test-repo', pushed_at: Math.floor(Date.now() / 1000) },
    commits: [
      {
        id: 'abc123def456abc123def456abc123def456abc1',
        message: 'E2E seed commit for push event',
        timestamp: new Date().toISOString(),
        author: { name: 'e2e-bot', email: 'e2e@test.local' },
      },
    ],
  };

  const secret = process.env.GITHUB_WEBHOOK_SECRET || 'dev-placeholder';
  const bodyString = JSON.stringify(payload);
  const hmac = crypto.createHmac('sha256', secret);
  hmac.update(bodyString);
  const signature = `sha256=${hmac.digest('hex')}`;

  const res = await fetch(`${API_BASE}/v1/ingest/github`, {
    method: 'POST',
    headers: {
      'Content-Type': 'application/json',
      'X-GitHub-Event': 'push',
      'X-GitHub-Delivery': crypto.randomUUID(),
      'X-Hub-Signature-256': signature,
      'X-Workspace-Id': workspaceId,
    },
    body: bodyString,
  });

  if (!res.ok && res.status !== 202) {
    const detail = await res.text();
    throw new Error(`seedGitHubEvent failed: ${res.status} — ${detail}`);
  }
}

// Polls GET /v1/memory until count > 0 or timeout
export async function waitForMemory(
  workspaceId: string,
  apiKey: string,
  timeoutMs = 15_000,
): Promise<void> {
  const start = Date.now();
  const pollInterval = 1_000;

  while (Date.now() - start < timeoutMs) {
    try {
      const res = await fetch(
        `${API_BASE}/v1/memory?workspace_id=${workspaceId}&limit=1`,
        {
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
      // Network error — retry
    }

    await new Promise((resolve) => setTimeout(resolve, pollInterval));
  }

  throw new Error(`waitForMemory timed out after ${timeoutMs}ms`);
}
