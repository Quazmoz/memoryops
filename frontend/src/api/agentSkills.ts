import { apiContractRequest, resolveOperationPath } from "./generated/contract";
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
  return apiContractRequest<AgentSkill[]>("listAgentSkills");
}

export async function getAgentSkill(assistant: "gemini" | "claude", name: string): Promise<AgentSkillContent> {
  return apiContractRequest<AgentSkillContent>("getAgentSkill", {
    path: resolveOperationPath("getAgentSkill", { assistant, name }),
  });
}

export async function createAgentSkill(payload: CreateAgentSkillPayload): Promise<AgentSkillContent> {
  return apiContractRequest<AgentSkillContent>("createAgentSkill", {
    body: payload as unknown as JsonValue,
  });
}

export async function updateAgentSkill(
  assistant: "gemini" | "claude",
  name: string,
  payload: UpdateAgentSkillPayload,
): Promise<AgentSkillContent> {
  return apiContractRequest<AgentSkillContent>("updateAgentSkill", {
    path: resolveOperationPath("updateAgentSkill", { assistant, name }),
    body: payload as unknown as JsonValue,
  });
}

export async function listAgentSkillVersions(assistant: "gemini" | "claude", name: string): Promise<AgentSkillVersion[]> {
  return apiContractRequest<AgentSkillVersion[]>("listAgentSkillVersions", {
    path: resolveOperationPath("listAgentSkillVersions", { assistant, name }),
  });
}

export async function getAgentSkillVersion(
  assistant: "gemini" | "claude",
  name: string,
  version: number,
): Promise<AgentSkillVersion> {
  return apiContractRequest<AgentSkillVersion>("getAgentSkillVersion", {
    path: resolveOperationPath("getAgentSkillVersion", { assistant, name, version }),
  });
}

export async function rollbackAgentSkillVersion(
  assistant: "gemini" | "claude",
  name: string,
  version: number,
  changeNote?: string | undefined,
): Promise<AgentSkillContent> {
  return apiContractRequest<AgentSkillContent>("rollbackAgentSkillVersion", {
    path: resolveOperationPath("rollbackAgentSkillVersion", { assistant, name, version }),
    body: changeNote ? { change_note: changeNote } : {},
  });
}
