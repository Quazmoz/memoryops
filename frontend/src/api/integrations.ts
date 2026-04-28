import { apiRequest } from "./client";
import type { DlqEntry, DlqEntryResponse, IntegrationResponse } from "./types";

export function listIntegrations(workspaceId: string): Promise<IntegrationResponse[]> {
  return apiRequest<IntegrationResponse[]>(`/v1/workspaces/${workspaceId}/integrations`);
}

export async function listDlq(workspaceId: string): Promise<DlqEntry[]> {
  const entries = await apiRequest<DlqEntryResponse[]>(`/v1/workspaces/${workspaceId}/dlq`);
  return entries.map((entry) => normalizeDlqEntry(entry, workspaceId));
}

export function retryDlqJob(workspaceId: string, jobId: string): Promise<void> {
  return apiRequest<void>(`/v1/workspaces/${workspaceId}/dlq/${jobId}/retry`, {
    method: "POST",
  });
}

export function discardDlqJob(workspaceId: string, jobId: string): Promise<void> {
  return apiRequest<void>(`/v1/workspaces/${workspaceId}/dlq/${jobId}`, {
    method: "DELETE",
  });
}

function normalizeDlqEntry(entry: DlqEntryResponse, workspaceId: string): DlqEntry {
  return {
    job_id: entry.job_id,
    workspace_id: entry.workspace_id ?? workspaceId,
    error_message: entry.error_message ?? entry.error ?? "",
    attempts: entry.attempts ?? entry.retry_count ?? 0,
    created_at: entry.created_at ?? entry.failed_at ?? null,
  };
}
