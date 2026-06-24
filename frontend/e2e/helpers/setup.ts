// Creates a test workspace + API key, returns { workspaceId, apiKey }
// Calls POST /v1/workspaces directly via fetch (no UI needed for setup)

const API_BASE = process.env.E2E_API_BASE ?? 'http://localhost:5173';

export async function createTestWorkspace(): Promise<{ workspaceId: string; apiKey: string }> {
  // 1. Create workspace
  const adminToken = process.env.WORKSPACE_CREATION_SECRET ?? process.env.X_ADMIN_TOKEN;
  const wsRes = await fetch(`${API_BASE}/v1/workspaces`, {
    method: 'POST',
    headers: {
      'Content-Type': 'application/json',
      ...(adminToken ? { 'x-admin-token': adminToken } : {}),
    },
    body: JSON.stringify({ name: `e2e-test-${Date.now()}-${Math.floor(Math.random() * 10000)}` }),
  });

  if (!wsRes.ok) {
    const detail = await wsRes.text();
    throw new Error(`Failed to create workspace: ${wsRes.status} — ${detail}`);
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

  // 2. Create github integration with dev-placeholder secret so webhooks succeed
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
