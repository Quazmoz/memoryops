import * as vscode from "vscode";

export interface MemoryOpsConfig {
  apiUrl: string;
  workspaceId: string;
  apiKey: string;
  defaultTopK: number;
  defaultTokenBudget: number;
  defaultAgentId: string;
}

export function getConfig(): MemoryOpsConfig {
  const config = vscode.workspace.getConfiguration("memoryops");
  return {
    apiUrl: trimTrailingSlash(config.get<string>("apiUrl", "http://localhost:8080")),
    workspaceId: config.get<string>("workspaceId", "").trim(),
    apiKey: config.get<string>("apiKey", "").trim(),
    defaultTopK: config.get<number>("defaultTopK", 5),
    defaultTokenBudget: config.get<number>("defaultTokenBudget", 2048),
    defaultAgentId: config.get<string>("defaultAgentId", "vscode").trim() || "vscode",
  };
}

export function validateConfig(config: MemoryOpsConfig): string[] {
  const missing: string[] = [];
  if (!config.apiUrl) {
    missing.push("memoryops.apiUrl");
  }
  if (!config.workspaceId) {
    missing.push("memoryops.workspaceId");
  }
  if (!config.apiKey) {
    missing.push("memoryops.apiKey");
  }
  return missing;
}

export async function openMemoryOpsSettings(): Promise<void> {
  await vscode.commands.executeCommand("workbench.action.openSettings", "@ext:quazmoz.memoryops-vscode memoryops");
}

function trimTrailingSlash(value: string): string {
  return value.trim().replace(/\/+$/, "");
}
