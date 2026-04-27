import { ApiError, apiRequest, extractDetail, parseResponse, queryString, requestHeaders } from "./client";
import type {
  AuditResponse,
  CreateApiKeyResponse,
  CreateWorkspaceResponse,
  DlqEntryResponse,
  IntegrationResponse,
} from "./types";

export type AuditParams = {
  limit?: number;
  offset?: number;
};

export function createWorkspace(name: string): Promise<CreateWorkspaceResponse> {
  return apiRequest<CreateWorkspaceResponse>("/v1/workspaces", {
    method: "POST",
    body: { name },
  });
}

export function createWorkspaceKey(workspaceId: string, name: string): Promise<CreateApiKeyResponse> {
  return apiRequest<CreateApiKeyResponse>(`/v1/workspaces/${workspaceId}/keys`, {
    method: "POST",
    body: { name },
  });
}

export function listAudit(workspaceId: string, params: AuditParams): Promise<AuditResponse> {
  return apiRequest<AuditResponse>(`/v1/workspaces/${workspaceId}/audit${queryString(params)}`);
}

export function listIntegrations(workspaceId: string): Promise<IntegrationResponse[]> {
  return apiRequest<IntegrationResponse[]>(`/v1/workspaces/${workspaceId}/integrations`);
}

export function listDlq(workspaceId: string): Promise<DlqEntryResponse[]> {
  return apiRequest<DlqEntryResponse[]>(`/v1/workspaces/${workspaceId}/dlq`);
}

export async function downloadWorkspaceExport(workspaceId: string): Promise<void> {
  const response = await fetch(`/v1/workspaces/${workspaceId}/export`, {
    headers: requestHeaders(),
  });
  const payload = response.ok ? null : await parseResponse(response);

  if (!response.ok) {
    throw new ApiError(response.status, extractDetail(payload, response.statusText));
  }

  const blob = await response.blob();
  const url = URL.createObjectURL(blob);
  const anchor = document.createElement("a");
  anchor.href = url;
  anchor.download = "memoryops-export.jsonl";
  document.body.append(anchor);
  anchor.click();
  anchor.remove();
  URL.revokeObjectURL(url);
}
