import * as vscode from "vscode";

export interface MemoryOpsConfig {
  apiUrl: string;
  workspaceId: string;
  apiKey: string;
  defaultTopK: number;
  defaultSearchMode: "hybrid" | "keyword" | "vector";
  defaultTokenBudget: number;
  sidebarPageSize: number;
  includeWorkspacePool: boolean;
  defaultAgentId: string;
}

export function getConfig(): MemoryOpsConfig {
  const config = vscode.workspace.getConfiguration("memoryops");
  return {
    apiUrl: trimTrailingSlash(config.get<string>("apiUrl", "http://localhost:8080")),
    workspaceId: config.get<string>("workspaceId", "").trim(),
    apiKey: config.get<string>("apiKey", "").trim(),
    defaultTopK: clampNumber(config.get<number>("defaultTopK", 5), 1, 20),
    defaultSearchMode: normalizeSearchMode(config.get<string>("defaultSearchMode", "hybrid")),
    defaultTokenBudget: clampNumber(config.get<number>("defaultTokenBudget", 2048), 256, 16000),
    sidebarPageSize: clampNumber(config.get<number>("sidebarPageSize", 20), 1, 100),
    includeWorkspacePool: config.get<boolean>("includeWorkspacePool", false),
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

function clampNumber(value: number, min: number, max: number): number {
  if (!Number.isFinite(value)) {
    return min;
  }

  return Math.min(Math.max(value, min), max);
}

function normalizeSearchMode(value: string): MemoryOpsConfig["defaultSearchMode"] {
  return value === "keyword" || value === "vector" ? value : "hybrid";
}
