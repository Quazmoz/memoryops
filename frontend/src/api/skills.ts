import { apiRequest } from "./client";
import type { JsonValue } from "./types";

export interface Skill {
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

export interface SkillVersion {
  id: string;
  skill_id: string;
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

export interface CreateSkillPayload {
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

export interface SkillInvocation {
  id: number;
  skill_id: string;
  workspace_id: string;
  skill_name: string;
  skill_version: number;
  actor: string;
  source: "http" | "mcp" | "test";
  status_code: number;
  latency_ms: number;
  error: string | null;
  occurred_at: string;
}

export interface ExportedSkill {
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

export interface ImportSkillsResponse {
  created: number;
  updated: number;
  skipped: number;
  errors: { name: string; error: string }[];
}

export async function listSkills(workspaceId: string): Promise<Skill[]> {
  return apiRequest<Skill[]>(`/v1/workspaces/${workspaceId}/skills`);
}

export async function createSkill(workspaceId: string, payload: CreateSkillPayload): Promise<Skill> {
  return apiRequest<Skill>(`/v1/workspaces/${workspaceId}/skills`, {
    method: "POST",
    body: skillPayload(payload),
  });
}

export async function updateSkill(workspaceId: string, name: string, patch: Partial<CreateSkillPayload>): Promise<Skill> {
  return apiRequest<Skill>(`/v1/workspaces/${workspaceId}/skills/${name}`, {
    method: "PATCH",
    body: skillPayload(patch),
  });
}

export async function deleteSkill(workspaceId: string, name: string): Promise<void> {
  await apiRequest<unknown>(`/v1/workspaces/${workspaceId}/skills/${name}`, {
    method: "DELETE",
  });
}

export interface SkillTestRequest {
  body?: JsonValue;
  headers?: Record<string, string>;
}

export interface SkillTestResponse {
  status: number;
  latency_ms: number;
  body: JsonValue;
}

export async function testSkill(workspaceId: string, name: string, request: SkillTestRequest): Promise<SkillTestResponse> {
  return apiRequest<SkillTestResponse>(`/v1/workspaces/${workspaceId}/skills/${name}/test`, {
    method: "POST",
    body: request as unknown as Record<string, JsonValue>,
  });
}

export async function listSkillVersions(workspaceId: string, name: string): Promise<SkillVersion[]> {
  return apiRequest<SkillVersion[]>(`/v1/workspaces/${workspaceId}/skills/${name}/versions`);
}

export async function getSkillVersion(workspaceId: string, name: string, version: number): Promise<SkillVersion> {
  return apiRequest<SkillVersion>(`/v1/workspaces/${workspaceId}/skills/${name}/versions/${version}`);
}

export async function rollbackSkillVersion(
  workspaceId: string,
  name: string,
  version: number,
  changeNote?: string,
): Promise<Skill> {
  return apiRequest<Skill>(`/v1/workspaces/${workspaceId}/skills/${name}/versions/${version}/rollback`, {
    method: "POST",
    body: changeNote ? { change_note: changeNote } : {},
  });
}

export async function invokeSkill(
  workspaceId: string,
  name: string,
  request: SkillTestRequest,
): Promise<SkillTestResponse> {
  return apiRequest<SkillTestResponse>(`/v1/workspaces/${workspaceId}/skills/${name}/invoke`, {
    method: "POST",
    body: request as unknown as Record<string, JsonValue>,
  });
}

export async function listSkillInvocations(
  workspaceId: string,
  name: string,
  limit = 50,
): Promise<SkillInvocation[]> {
  return apiRequest<SkillInvocation[]>(
    `/v1/workspaces/${workspaceId}/skills/${name}/invocations?limit=${limit}`,
  );
}

export async function exportSkills(workspaceId: string): Promise<ExportedSkill[]> {
  return apiRequest<ExportedSkill[]>(`/v1/workspaces/${workspaceId}/skills/export`);
}

export async function importSkills(
  workspaceId: string,
  skills: unknown[],
  overwrite = false,
): Promise<ImportSkillsResponse> {
  return apiRequest<ImportSkillsResponse>(`/v1/workspaces/${workspaceId}/skills/import`, {
    method: "POST",
    body: { skills, overwrite } as Record<string, JsonValue>,
  });
}

function skillPayload(payload: Partial<CreateSkillPayload>): Record<string, JsonValue> {
  return Object.fromEntries(
    Object.entries(payload)
      .filter(([, value]) => value !== undefined)
      .map(([key, value]) => [key, value as JsonValue]),
  );
}
