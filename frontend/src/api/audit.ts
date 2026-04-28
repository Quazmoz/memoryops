import { apiRequest, queryString } from "./client";
import type { AuditEvent, AuditResponse } from "./types";

export async function listAuditEvents(workspaceId: string, limit: number, offset: number): Promise<AuditEvent[]> {
  const response = await apiRequest<AuditResponse>(`/v1/workspaces/${workspaceId}/audit${queryString({ limit, offset })}`);
  return response.items;
}
