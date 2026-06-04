import { apiRequest } from "./client";
import type { JsonValue } from "./types";

export interface Tool {
  id: string;
  workspace_id: string;
  name: string;
  description: string;
  endpoint_url: string;
  http_method: string;
  input_schema: unknown;
  output_schema: unknown;
  auth_header: string | null;
  enabled: boolean;
  version: number;
  scope_visibility: "private" | "workspace";
  rate_limit_per_minute: number;
  circuit_breaker_threshold: number;
  circuit_breaker_cooldown_seconds: number;
  created_at: string;
  updated_at: string;
}

export interface ToolVersion {
  id: string;
  tool_id: string;
  workspace_id: string;
  name: string;
  version: number;
  description: string;
  endpoint_url: string;
  http_method: string;
  input_schema: unknown;
  output_schema: unknown;
  auth_header: string | null;
  enabled: boolean;
  scope_visibility: "private" | "workspace";
  change_note: string | null;
  created_by: string | null;
  created_at: string;
}

export interface CreateToolPayload {
  name: string;
  description: string;
  endpoint_url: string;
  http_method?: string;
  input_schema?: unknown;
  output_schema?: unknown;
  auth_header?: string;
  auth_secret?: string;
  enabled?: boolean;
  change_note?: string;
  scope_visibility?: "private" | "workspace";
  rate_limit_per_minute?: number;
  circuit_breaker_threshold?: number;
  circuit_breaker_cooldown_seconds?: number;
}

export interface ToolInvocation {
  id: number;
  tool_id: string;
  workspace_id: string;
  tool_name: string;
  tool_version: number;
  actor: string;
  source: "http" | "mcp" | "test";
  status_code: number;
  latency_ms: number;
  error: string | null;
  occurred_at: string;
}

export interface ExportedTool {
  name: string;
  description: string;
  endpoint_url: string;
  http_method: string;
  input_schema: unknown;
  output_schema: unknown;
  auth_header: string | null;
  enabled: boolean;
  scope_visibility: "private" | "workspace";
  rate_limit_per_minute: number;
  circuit_breaker_threshold: number;
  circuit_breaker_cooldown_seconds: number;
  version: number;
}

export interface ImportToolsResponse {
  created: number;
  updated: number;
  skipped: number;
  errors: { name: string; error: string }[];
}

export async function listTools(workspaceId: string): Promise<Tool[]> {
  return apiRequest<Tool[]>(`/v1/workspaces/${workspaceId}/tools`);
}

export async function createTool(workspaceId: string, payload: CreateToolPayload): Promise<Tool> {
  return apiRequest<Tool>(`/v1/workspaces/${workspaceId}/tools`, {
    method: "POST",
    body: toolPayload(payload),
  });
}

export async function updateTool(workspaceId: string, name: string, patch: Partial<CreateToolPayload>): Promise<Tool> {
  return apiRequest<Tool>(`/v1/workspaces/${workspaceId}/tools/${name}`, {
    method: "PATCH",
    body: toolPayload(patch),
  });
}

export async function deleteTool(workspaceId: string, name: string): Promise<void> {
  await apiRequest<unknown>(`/v1/workspaces/${workspaceId}/tools/${name}`, {
    method: "DELETE",
  });
}

export interface ToolTestRequest {
  body?: JsonValue;
  headers?: Record<string, string>;
}

export interface ToolTestResponse {
  status: number;
  latency_ms: number;
  body: JsonValue;
}

export async function testTool(workspaceId: string, name: string, request: ToolTestRequest): Promise<ToolTestResponse> {
  return apiRequest<ToolTestResponse>(`/v1/workspaces/${workspaceId}/tools/${name}/test`, {
    method: "POST",
    body: request as unknown as Record<string, JsonValue>,
  });
}

export async function listToolVersions(workspaceId: string, name: string): Promise<ToolVersion[]> {
  return apiRequest<ToolVersion[]>(`/v1/workspaces/${workspaceId}/tools/${name}/versions`);
}

export async function getToolVersion(workspaceId: string, name: string, version: number): Promise<ToolVersion> {
  return apiRequest<ToolVersion>(`/v1/workspaces/${workspaceId}/tools/${name}/versions/${version}`);
}

export async function rollbackToolVersion(
  workspaceId: string,
  name: string,
  version: number,
  changeNote?: string,
): Promise<Tool> {
  return apiRequest<Tool>(`/v1/workspaces/${workspaceId}/tools/${name}/versions/${version}/rollback`, {
    method: "POST",
    body: changeNote ? { change_note: changeNote } : {},
  });
}

export async function invokeTool(
  workspaceId: string,
  name: string,
  request: ToolTestRequest,
): Promise<ToolTestResponse> {
  return apiRequest<ToolTestResponse>(`/v1/workspaces/${workspaceId}/tools/${name}/invoke`, {
    method: "POST",
    body: request as unknown as Record<string, JsonValue>,
  });
}

export async function listToolInvocations(
  workspaceId: string,
  name: string,
  limit = 50,
): Promise<ToolInvocation[]> {
  return apiRequest<ToolInvocation[]>(
    `/v1/workspaces/${workspaceId}/tools/${name}/invocations?limit=${limit}`,
  );
}

export async function exportTools(workspaceId: string): Promise<ExportedTool[]> {
  return apiRequest<ExportedTool[]>(`/v1/workspaces/${workspaceId}/tools/export`);
}

export async function importTools(
  workspaceId: string,
  tools: unknown[],
  overwrite = false,
): Promise<ImportToolsResponse> {
  return apiRequest<ImportToolsResponse>(`/v1/workspaces/${workspaceId}/tools/import`, {
    method: "POST",
    body: { tools, overwrite } as Record<string, JsonValue>,
  });
}

function toolPayload(payload: Partial<CreateToolPayload>): Record<string, JsonValue> {
  return Object.fromEntries(
    Object.entries(payload)
      .filter(([, value]) => value !== undefined)
      .map(([key, value]) => [key, value as JsonValue]),
  );
}
