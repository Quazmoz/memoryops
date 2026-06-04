import { apiRequest } from "./client";

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
  content: string;
}

export async function listAgentSkills(): Promise<AgentSkill[]> {
  return apiRequest<AgentSkill[]>("/v1/agent-skills");
}

export async function getAgentSkill(assistant: "gemini" | "claude", name: string): Promise<AgentSkillContent> {
  return apiRequest<AgentSkillContent>(`/v1/agent-skills/${assistant}/${name}`);
}
