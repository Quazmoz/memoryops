import { ApiError, apiRequest, extractDetail, parseResponse, requestHeaders } from "./client";
import type { CreatedApiKey, CreateApiKeyResponse, CreateWorkspaceResponse, WorkspaceSummary } from "./types";

export async function createWorkspace(name: string): Promise<WorkspaceSummary> {
  const response = await apiRequest<CreateWorkspaceResponse>("/v1/workspaces", {
    method: "POST",
    auth: false,
    body: { name },
  });
  const id = response.id ?? response.workspace_id;

  if (!id) {
    throw new Error("Workspace response did not include an id");
  }

  return {
    id,
    name: response.name ?? name,
  };
}

export async function createApiKey(workspaceId: string, name: string): Promise<CreatedApiKey> {
  const response = await apiRequest<CreateApiKeyResponse>(`/v1/workspaces/${workspaceId}/keys`, {
    method: "POST",
    auth: false,
    body: { name },
  });
  const plaintextKey = response.plaintext_key ?? response.key;

  if (!plaintextKey) {
    throw new Error("API key response did not include a plaintext key");
  }

  return { plaintext_key: plaintextKey };
}

export async function exportMemories(workspaceId: string): Promise<Blob> {
  const response = await fetch(`/v1/workspaces/${workspaceId}/export`, {
    headers: requestHeaders(),
  });
  const payload = response.ok ? null : await parseResponse(response);

  if (!response.ok) {
    throw new ApiError(response.status, extractDetail(payload, response.statusText));
  }

  return response.blob();
}
