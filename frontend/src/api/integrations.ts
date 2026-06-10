import { apiRequest } from "./client";
import type { IntegrationResponse, JsonValue } from "./types";

export type DlqJob = {
  id: string;
  workspace_id: string;
  source: string;
  error_message: string;
  payload: JsonValue;
  failed_at: string;
  retry_count: number;
};

type DlqJobResponse = {
  id?: string;
  job_id?: string;
  workspace_id?: string;
  source?: string;
  error_message?: string;
  error?: string;
  payload?: JsonValue;
  payload_summary?: string;
  failed_at?: string | null;
  retry_count?: number;
  attempts?: number;
  created_at?: string | null;
};

/** Source values accepted by the backend `Source` enum (serde lowercase). */
export const INTEGRATION_SOURCES = ["github", "slack", "jira", "linear", "observation"] as const;

export type IntegrationSource = (typeof INTEGRATION_SOURCES)[number];

export type CreateIntegrationRequest = {
  source: IntegrationSource;
  webhook_secret: string;
};

export function listIntegrations(workspaceId: string): Promise<IntegrationResponse[]> {
  return apiRequest<IntegrationResponse[]>(`/v1/workspaces/${encodeURIComponent(workspaceId)}/integrations`);
}

export function createIntegration(workspaceId: string, request: CreateIntegrationRequest): Promise<IntegrationResponse> {
  const webhookSecret = request.webhook_secret.trim();
  if (webhookSecret.length === 0) {
    return Promise.reject(new Error("Webhook secret is required"));
  }

  return apiRequest<IntegrationResponse>(`/v1/workspaces/${encodeURIComponent(workspaceId)}/integrations`, {
    method: "POST",
    body: { source: request.source, webhook_secret: webhookSecret },
  });
}

export function deleteIntegration(workspaceId: string, source: string): Promise<void> {
  return apiRequest<void>(
    `/v1/workspaces/${encodeURIComponent(workspaceId)}/integrations/${encodeURIComponent(source)}`,
    { method: "DELETE" },
  );
}

export async function listDlqJobs(workspaceId: string): Promise<DlqJob[]> {
  const entries = await apiRequest<DlqJobResponse[]>(`/v1/workspaces/${workspaceId}/dlq`);
  return entries.map((entry) => normalizeDlqJob(entry, workspaceId)).filter((job) => job.id.length > 0);
}

export const listDlq = listDlqJobs;

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

function normalizeDlqJob(entry: DlqJobResponse, workspaceId: string): DlqJob {
  const payload = normalizePayload(entry);

  return {
    id: entry.id ?? entry.job_id ?? "",
    workspace_id: entry.workspace_id ?? workspaceId,
    source: entry.source ?? inferSource(payload),
    error_message: entry.error_message ?? entry.error ?? "",
    payload,
    failed_at: entry.failed_at ?? entry.created_at ?? "",
    retry_count: entry.retry_count ?? entry.attempts ?? 0,
  };
}

function normalizePayload(entry: DlqJobResponse): JsonValue {
  if (entry.payload !== undefined) {
    return entry.payload;
  }

  const summary = entry.payload_summary?.trim();
  if (!summary) {
    return {};
  }

  try {
    const parsed = JSON.parse(summary) as unknown;
    if (isJsonValue(parsed)) {
      return parsed;
    }
  } catch {
    return { summary };
  }

  return { summary };
}

function inferSource(payload: JsonValue): string {
  if (!isJsonRecord(payload)) {
    return "unknown";
  }

  const type = stringField(payload, "type") ?? "";
  const eventKind = stringField(payload, "event_kind") ?? "";
  const webhookEvent = stringField(payload, "webhook_event") ?? "";

  if (eventKind.startsWith("linear.") || ((type === "Issue" || type === "Comment") && eventKind.length > 0)) {
    return "linear";
  }

  if (webhookEvent.startsWith("jira:") || webhookEvent.startsWith("comment_") || type.startsWith("jira:") || stringField(payload, "issue_key")) {
    return "jira";
  }

  if (["message", "message.edited", "app_mention", "reaction_added"].includes(type)) {
    return "slack";
  }

  if ("repository" in payload || "pull_request" in payload || "commits" in payload) {
    return "github";
  }

  if (stringField(payload, "memory_id")) {
    return "processor";
  }

  return "unknown";
}

function stringField(record: Record<string, JsonValue>, key: string): string | null {
  const value = record[key];
  return typeof value === "string" && value.trim().length > 0 ? value : null;
}

function isJsonValue(value: unknown): value is JsonValue {
  if (value === null || typeof value === "string" || typeof value === "number" || typeof value === "boolean") {
    return true;
  }

  if (Array.isArray(value)) {
    return value.every(isJsonValue);
  }

  if (typeof value === "object") {
    return Object.values(value).every(isJsonValue);
  }

  return false;
}

function isJsonRecord(value: JsonValue): value is Record<string, JsonValue> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}
