import { apiRequest, apiUrl, queryString, requestHeaders } from "./client";
import type {
  AuditActionsResponse,
  AuditChainVerification,
  AuditEvent,
  AuditResponse,
} from "./types";

/** Investigation filters shared by the list and export endpoints. */
export type AuditFilters = {
  actor?: string;
  /** Comma-separated list of actions. */
  actions?: string;
  target_type?: string;
  target_id?: string;
  target_name?: string;
  request_id?: string;
  correlation_id?: string;
  source_ip?: string;
  /** Comma-separated severities. */
  severity?: string;
  /** Comma-separated categories. */
  category?: string;
  success?: boolean;
  from?: string;
  to?: string;
  q?: string;
};

export type AuditListParams = AuditFilters & {
  limit: number;
  cursor?: string | null;
};

export async function listAuditEvents(workspaceId: string, params: AuditListParams): Promise<AuditResponse> {
  const { limit, cursor, ...filters } = params;
  return apiRequest<AuditResponse>(
    `/v1/workspaces/${workspaceId}/audit${queryString({ limit, after: cursor, ...filters })}`,
  );
}

export async function getAuditEntry(workspaceId: string, auditId: string): Promise<AuditEvent> {
  return apiRequest<AuditEvent>(`/v1/workspaces/${workspaceId}/audit/${auditId}`);
}

export async function listAuditActions(workspaceId: string): Promise<AuditActionsResponse> {
  return apiRequest<AuditActionsResponse>(`/v1/workspaces/${workspaceId}/audit/actions`);
}

export async function verifyAuditChain(workspaceId: string): Promise<AuditChainVerification> {
  return apiRequest<AuditChainVerification>(`/v1/workspaces/${workspaceId}/audit/verify`, {
    method: "POST",
  });
}

/** URL for the export endpoint (used to open/download in the browser). */
export function auditExportUrl(workspaceId: string, format: "jsonl" | "csv", filters: AuditFilters): string {
  return apiUrl(`/v1/workspaces/${workspaceId}/audit/export${queryString({ format, ...filters })}`);
}

/**
 * Download an audit export. Uses fetch with the API key header (the export
 * route is authenticated) and triggers a client-side file download.
 */
export async function downloadAuditExport(
  workspaceId: string,
  format: "jsonl" | "csv",
  filters: AuditFilters,
): Promise<void> {
  const response = await fetch(auditExportUrl(workspaceId, format, filters), {
    headers: requestHeaders({}, true, false),
  });
  if (!response.ok) {
    throw new Error(`Export failed (${response.status})`);
  }
  const blob = await response.blob();
  const url = URL.createObjectURL(blob);
  const link = document.createElement("a");
  link.href = url;
  link.download = `audit-export.${format}`;
  document.body.appendChild(link);
  link.click();
  link.remove();
  URL.revokeObjectURL(url);
}
