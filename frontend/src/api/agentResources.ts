import { apiRequest, queryString } from "./client";
import type { JsonValue } from "./types";

export type AgentResourceKind = "skill" | "agent" | "prompt" | "instruction";
export type AgentResourceAssistant = "generic" | "openai" | "claude" | "gemini";

export interface AgentResourceSummary {
  id: string;
  workspace_id: string;
  kind: AgentResourceKind;
  assistant: AgentResourceAssistant;
  name: string;
  filename: string;
  title: string;
  description: string;
  metadata: Record<string, JsonValue>;
  version: number;
  created_at: string;
  updated_at: string;
}

export interface AgentResource extends AgentResourceSummary {
  body: string;
  content: string;
}

export interface AgentResourceVersion {
  id: string;
  resource_id: string;
  workspace_id: string;
  kind: AgentResourceKind;
  assistant: AgentResourceAssistant;
  name: string;
  filename: string;
  title: string;
  description: string;
  body: string;
  content: string;
  metadata: Record<string, JsonValue>;
  version: number;
  change_note: string | null;
  created_by: string | null;
  created_at: string;
}

export interface CreateAgentResourcePayload {
  kind: AgentResourceKind;
  assistant?: AgentResourceAssistant;
  name: string;
  title: string;
  description: string;
  body: string;
  content?: string;
  metadata?: Record<string, JsonValue>;
  change_note?: string;
}

export type UpdateAgentResourcePayload = Partial<
  Pick<CreateAgentResourcePayload, "title" | "description" | "body" | "content" | "metadata" | "change_note">
>;

export interface AgentResourceListFilters {
  kind?: AgentResourceKind;
  assistant?: AgentResourceAssistant;
}

export async function listAgentResources(filters: AgentResourceListFilters = {}): Promise<AgentResourceSummary[]> {
  return apiRequest<AgentResourceSummary[]>(
    `/v1/agent-resources${queryString({ kind: filters.kind, assistant: filters.assistant })}`,
  );
}

export async function getAgentResource(
  kind: AgentResourceKind,
  assistant: AgentResourceAssistant,
  name: string,
): Promise<AgentResource> {
  return apiRequest<AgentResource>(agentResourcePath(kind, assistant, name));
}

export async function createAgentResource(payload: CreateAgentResourcePayload): Promise<AgentResource> {
  return apiRequest<AgentResource>("/v1/agent-resources", {
    method: "POST",
    body: resourcePayload(payload),
  });
}

export async function updateAgentResource(
  kind: AgentResourceKind,
  assistant: AgentResourceAssistant,
  name: string,
  payload: UpdateAgentResourcePayload,
): Promise<AgentResource> {
  return apiRequest<AgentResource>(agentResourcePath(kind, assistant, name), {
    method: "PUT",
    body: resourcePayload(payload),
  });
}

export async function deleteAgentResource(
  kind: AgentResourceKind,
  assistant: AgentResourceAssistant,
  name: string,
): Promise<void> {
  await apiRequest<unknown>(agentResourcePath(kind, assistant, name), {
    method: "DELETE",
  });
}

export async function listAgentResourceVersions(
  kind: AgentResourceKind,
  assistant: AgentResourceAssistant,
  name: string,
): Promise<AgentResourceVersion[]> {
  return apiRequest<AgentResourceVersion[]>(`${agentResourcePath(kind, assistant, name)}/versions`);
}

export async function getAgentResourceVersion(
  kind: AgentResourceKind,
  assistant: AgentResourceAssistant,
  name: string,
  version: number,
): Promise<AgentResourceVersion> {
  return apiRequest<AgentResourceVersion>(`${agentResourcePath(kind, assistant, name)}/versions/${version}`);
}

export async function rollbackAgentResource(
  kind: AgentResourceKind,
  assistant: AgentResourceAssistant,
  name: string,
  version: number,
  changeNote?: string,
): Promise<AgentResource> {
  return apiRequest<AgentResource>(`${agentResourcePath(kind, assistant, name)}/versions/${version}/rollback`, {
    method: "POST",
    body: changeNote ? { change_note: changeNote } : {},
  });
}

function agentResourcePath(kind: AgentResourceKind, assistant: AgentResourceAssistant, name: string): string {
  return `/v1/agent-resources/${kind}/${assistant}/${encodeURIComponent(name)}`;
}

function resourcePayload(payload: UpdateAgentResourcePayload | CreateAgentResourcePayload): Record<string, JsonValue> {
  return Object.fromEntries(
    Object.entries(payload)
      .filter(([, value]) => value !== undefined)
      .map(([key, value]) => [key, value as JsonValue]),
  );
}
