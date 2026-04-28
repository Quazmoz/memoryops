import { ApiError, apiRequest, extractDetail, parseResponse, requestHeaders } from "./client";
import type {
  CreatedApiKey,
  CreateApiKeyResponse,
  CreateWorkspaceResponse,
  JsonValue,
  PromotionReport,
  WorkspaceConfig,
  WorkspaceDetail,
  WorkspaceSummary,
} from "./types";

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

export function getWorkspace(workspaceId: string): Promise<WorkspaceDetail> {
  return apiRequest<WorkspaceDetail>(`/v1/workspaces/${workspaceId}`);
}

export function updateWorkspaceConfig(workspaceId: string, patch: WorkspaceConfig): Promise<WorkspaceDetail> {
  return apiRequest<WorkspaceDetail>(`/v1/workspaces/${workspaceId}/config`, {
    method: "PATCH",
    body: configPatchBody(patch),
  });
}

export function triggerPromotion(workspaceId: string): Promise<PromotionReport> {
  return apiRequest<PromotionReport>(`/v1/workspaces/${workspaceId}/promote`, {
    method: "POST",
  });
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

function configPatchBody(patch: WorkspaceConfig): { [key: string]: JsonValue } {
  const body: { [key: string]: JsonValue } = {};

  Object.entries(patch).forEach(([key, value]) => {
    if (value !== undefined) {
      body[key] = value;
    }
  });

  return body;
}
