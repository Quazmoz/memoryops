import { apiRequest, queryString } from "./client";
import type { AuditResponse } from "./types";

export type AuditListParams = {
  limit: number;
  cursor?: string | null;
};

export async function listAuditEvents(workspaceId: string, params: AuditListParams): Promise<AuditResponse> {
  return apiRequest<AuditResponse>(
    `/v1/workspaces/${workspaceId}/audit${queryString({ limit: params.limit, after: params.cursor })}`,
  );
}
