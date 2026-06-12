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
  scope_visibility: "private" | "workspace" | "published";
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
  scope_visibility: "private" | "workspace" | "published";
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
  scope_visibility?: "private" | "workspace" | "published";
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

export interface ToolSecret {
  auth_header: string | null;
  plaintext_secret: string;
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
  scope_visibility: "private" | "workspace" | "published";
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

export async function getTool(workspaceId: string, name: string): Promise<Tool> {
  return apiRequest<Tool>(toolPath(workspaceId, name));
}

export async function getToolSecret(workspaceId: string, name: string): Promise<ToolSecret> {
  return apiRequest<ToolSecret>(`${toolPath(workspaceId, name)}/secret`);
}

export async function createTool(workspaceId: string, payload: CreateToolPayload): Promise<Tool> {
  return apiRequest<Tool>(`/v1/workspaces/${workspaceId}/tools`, {
    method: "POST",
    body: toolPayload(payload),
  });
}

export async function updateTool(workspaceId: string, name: string, patch: Partial<CreateToolPayload>): Promise<Tool> {
  return apiRequest<Tool>(toolPath(workspaceId, name), {
    method: "PATCH",
    body: toolPayload(patch),
  });
}

export async function deleteTool(workspaceId: string, name: string): Promise<void> {
  await apiRequest<unknown>(toolPath(workspaceId, name), {
    method: "DELETE",
  });
}

export interface ToolTestRequest {
  body?: JsonValue;
  headers?: Record<string, string>;
  version?: number;
}

export interface ToolTestResponse {
  status: number;
  latency_ms: number;
  body: JsonValue;
}

export async function testTool(workspaceId: string, name: string, request: ToolTestRequest): Promise<ToolTestResponse> {
  return apiRequest<ToolTestResponse>(`${toolPath(workspaceId, name)}/test`, {
    method: "POST",
    body: request as unknown as Record<string, JsonValue>,
  });
}

export async function listToolVersions(workspaceId: string, name: string): Promise<ToolVersion[]> {
  return apiRequest<ToolVersion[]>(`${toolPath(workspaceId, name)}/versions`);
}

export async function getToolVersion(workspaceId: string, name: string, version: number): Promise<ToolVersion> {
  return apiRequest<ToolVersion>(`${toolPath(workspaceId, name)}/versions/${version}`);
}

export async function rollbackToolVersion(
  workspaceId: string,
  name: string,
  version: number,
  changeNote?: string,
): Promise<Tool> {
  return apiRequest<Tool>(`${toolPath(workspaceId, name)}/versions/${version}/rollback`, {
    method: "POST",
    body: changeNote ? { change_note: changeNote } : {},
  });
}

export async function invokeTool(
  workspaceId: string,
  name: string,
  request: ToolTestRequest,
): Promise<ToolTestResponse> {
  return apiRequest<ToolTestResponse>(`${toolPath(workspaceId, name)}/invoke`, {
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
    `${toolPath(workspaceId, name)}/invocations?limit=${limit}`,
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

function toolPath(workspaceId: string, name: string): string {
  return `/v1/workspaces/${workspaceId}/tools/${encodeURIComponent(name)}`;
}

function toolPayload(payload: Partial<CreateToolPayload>): Record<string, JsonValue> {
  return Object.fromEntries(
    Object.entries(payload)
      .filter(([, value]) => value !== undefined)
      .map(([key, value]) => [key, value as JsonValue]),
  );
}
