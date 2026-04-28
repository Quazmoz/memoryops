// Creates a test workspace + API key, returns { workspaceId, apiKey }
// Calls POST /v1/workspaces directly via fetch (no UI needed for setup)

const API_BASE = process.env.E2E_API_BASE ?? 'http://localhost:5173';

export async function createTestWorkspace(): Promise<{ workspaceId: string; apiKey: string }> {
  // 1. Create workspace
  const wsRes = await fetch(`${API_BASE}/v1/workspaces`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ name: `e2e-test-${Date.now()}-${Math.floor(Math.random() * 10000)}` }),
  });

  if (!wsRes.ok) {
    const detail = await wsRes.text();
    throw new Error(`Failed to create workspace: ${wsRes.status} — ${detail}`);
  }

  const wsData = (await wsRes.json()) as { id?: string; workspace_id?: string };
  const workspaceId = wsData.id ?? wsData.workspace_id;
  if (!workspaceId) {
    throw new Error('Workspace response did not include an id');
  }

  // 2. Create API key
  const keyRes = await fetch(`${API_BASE}/v1/workspaces/${workspaceId}/keys`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ name: 'e2e-default' }),
  });

  if (!keyRes.ok) {
    const detail = await keyRes.text();
    throw new Error(`Failed to create API key: ${keyRes.status} — ${detail}`);
  }

  const keyData = (await keyRes.json()) as { plaintext_key?: string; key?: string };
  const apiKey = keyData.plaintext_key ?? keyData.key;
  if (!apiKey) {
    throw new Error('API key response did not include a plaintext key');
  }

  return { workspaceId, apiKey };
}
