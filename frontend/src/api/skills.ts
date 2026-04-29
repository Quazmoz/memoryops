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
  created_at: string;
  updated_at: string;
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

function skillPayload(payload: Partial<CreateSkillPayload>): Record<string, JsonValue> {
  return Object.fromEntries(
    Object.entries(payload)
      .filter(([, value]) => value !== undefined)
      .map(([key, value]) => [key, value as JsonValue]),
  );
}
