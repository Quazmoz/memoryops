// Returns a workspace + API key for full-stack E2E specs.
// The current first-run product flow uses the built-in default workspace, so
// E2E does the same and avoids exhausting workspace-creation rate limits.

const API_BASE = process.env.E2E_API_BASE ?? 'http://localhost:5173';

let workspacePromise: Promise<{ workspaceId: string; apiKey: string }> | undefined;

export async function createTestWorkspace(): Promise<{ workspaceId: string; apiKey: string }> {
  workspacePromise ??= loadDefaultWorkspace();
  return workspacePromise;
}

async function loadDefaultWorkspace(): Promise<{ workspaceId: string; apiKey: string }> {
  const wsRes = await fetch(`${API_BASE}/v1/default-workspace`);

  if (!wsRes.ok) {
    const detail = await wsRes.text();
    throw new Error(`Failed to load default workspace: ${wsRes.status} — ${detail}`);
  }

  const wsData = (await wsRes.json()) as { id?: string; workspace_id?: string; api_key?: string };
  const workspaceId = wsData.workspace_id ?? wsData.id;
  const apiKey = wsData.api_key;
  if (!workspaceId) {
    throw new Error('Workspace response did not include an id');
  }
  if (!apiKey) {
    throw new Error('Workspace response did not include an api_key');
  }

  // Ensure GitHub webhooks are accepted by ingest-focused specs. This endpoint
  // upserts by workspace/source, so repeated calls across workers are harmless.
  const integrationRes = await fetch(`${API_BASE}/v1/workspaces/${workspaceId}/integrations`, {
    method: 'POST',
    headers: {
      'Content-Type': 'application/json',
      'x-api-key': apiKey,
    },
    body: JSON.stringify({
      source: 'github',
      webhook_secret: 'dev-placeholder',
    }),
  });

  if (!integrationRes.ok) {
    const detail = await integrationRes.text();
    throw new Error(`Failed to create integration: ${integrationRes.status} — ${detail}`);
  }

  return { workspaceId, apiKey };
}
