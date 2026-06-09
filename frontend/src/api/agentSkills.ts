import { apiRequest } from "./client";
import type { JsonValue } from "./types";

export interface AgentSkill {
  id: string;
  name: string;
  filename: string;
  assistant: "gemini" | "claude";
  title: string;
  description: string;
  version: number;
}

export interface AgentSkillContent {
  id: string;
  name: string;
  filename: string;
  assistant: "gemini" | "claude";
  title: string;
  description: string;
  instructions: string;
  content: string;
  version: number;
}

export interface AgentSkillVersion {
  id: string;
  agent_skill_id: string;
  workspace_id: string;
  name: string;
  version: number;
  assistant: "gemini" | "claude";
  title: string;
  description: string;
  instructions: string;
  content: string;
  change_note: string | null;
  created_by: string | null;
  created_at: string;
}

export interface CreateAgentSkillPayload {
  assistant: "gemini" | "claude";
  name: string;
  title: string;
  description: string;
  instructions: string;
  change_note?: string | undefined;
}

export interface UpdateAgentSkillPayload {
  title: string;
  description: string;
  instructions: string;
  change_note?: string | undefined;
}

export async function listAgentSkills(): Promise<AgentSkill[]> {
  return apiRequest<AgentSkill[]>("/v1/agent-skills");
}

export async function getAgentSkill(assistant: "gemini" | "claude", name: string): Promise<AgentSkillContent> {
  return apiRequest<AgentSkillContent>(`/v1/agent-skills/${assistant}/${name}`);
}

export async function createAgentSkill(payload: CreateAgentSkillPayload): Promise<AgentSkillContent> {
  return apiRequest<AgentSkillContent>("/v1/agent-skills", {
    method: "POST",
    body: payload as unknown as JsonValue,
  });
}

export async function updateAgentSkill(
  assistant: "gemini" | "claude",
  name: string,
  payload: UpdateAgentSkillPayload,
): Promise<AgentSkillContent> {
  return apiRequest<AgentSkillContent>(`/v1/agent-skills/${assistant}/${name}`, {
    method: "PUT",
    body: payload as unknown as JsonValue,
  });
}

export async function listAgentSkillVersions(assistant: "gemini" | "claude", name: string): Promise<AgentSkillVersion[]> {
  return apiRequest<AgentSkillVersion[]>(`/v1/agent-skills/${assistant}/${name}/versions`);
}

export async function getAgentSkillVersion(
  assistant: "gemini" | "claude",
  name: string,
  version: number,
): Promise<AgentSkillVersion> {
  return apiRequest<AgentSkillVersion>(`/v1/agent-skills/${assistant}/${name}/versions/${version}`);
}

export async function rollbackAgentSkillVersion(
  assistant: "gemini" | "claude",
  name: string,
  version: number,
  changeNote?: string | undefined,
): Promise<AgentSkillContent> {
  return apiRequest<AgentSkillContent>(`/v1/agent-skills/${assistant}/${name}/versions/${version}/rollback`, {
    method: "POST",
    body: changeNote ? { change_note: changeNote } : {},
  });
}
