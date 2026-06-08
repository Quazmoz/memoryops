import { apiRequest } from "./client";
import type { JsonValue } from "./types";

export interface AgentSkill {
  name: string;
  filename: string;
  assistant: "gemini" | "claude";
  title: string;
  description: string;
}

export interface AgentSkillContent {
  name: string;
  filename: string;
  assistant: "gemini" | "claude";
  title: string;
  description: string;
  instructions: string;
  content: string;
}

export interface CreateAgentSkillPayload {
  assistant: "gemini" | "claude";
  name: string;
  title: string;
  description: string;
  instructions: string;
}

export interface UpdateAgentSkillPayload {
  title: string;
  description: string;
  instructions: string;
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
