import { ApiError, apiRequest, extractDetail, parseResponse, requestHeaders } from "./client";
import type {
  CreatedApiKey,
  CreateApiKeyResponse,
  CreateWorkspaceResponse,
  JsonValue,
  PromotionReport,
  WorkspaceConfig,
  WorkspaceDetail,
  WorkspaceSummary,
  WorkspaceStats,
} from "./types";

export async function createWorkspace(name: string): Promise<WorkspaceSummary> {
  const response = await apiRequest<CreateWorkspaceResponse>("/v1/workspaces", {
    method: "POST",
    auth: false,
    body: { name },
  });
  const id = response.id ?? response.workspace_id;

  if (!id) {
    throw new Error("Workspace response did not include an id");
  }

  return {
    id,
    name: response.name ?? name,
  };
}

export async function createApiKey(workspaceId: string, name: string): Promise<CreatedApiKey> {
  const response = await apiRequest<CreateApiKeyResponse>(`/v1/workspaces/${workspaceId}/keys`, {
    method: "POST",
    auth: false,
    body: { name },
  });
  const plaintextKey = response.plaintext_key ?? response.key;

  if (!plaintextKey) {
    throw new Error("API key response did not include a plaintext key");
  }

  return { plaintext_key: plaintextKey };
}

export function getWorkspace(workspaceId: string): Promise<WorkspaceDetail> {
  return apiRequest<WorkspaceDetail>(`/v1/workspaces/${workspaceId}`).then(normalizeWorkspaceDetail);
}

export function getWorkspaceStats(workspaceId: string): Promise<WorkspaceStats> {
  return apiRequest<WorkspaceStats>(`/v1/workspaces/${workspaceId}/stats`);
}

export type StatsHistoryPoint = {
  date: string;
  created: number;
  promoted: number;
  soft_deleted: number;
};

export type StatsHistory = {
  days: number;
  series: StatsHistoryPoint[];
};

export function getWorkspaceStatsHistory(workspaceId: string, days = 30): Promise<StatsHistory> {
  return apiRequest<StatsHistory>(`/v1/workspaces/${workspaceId}/stats/history?days=${days}`);
}

export function updateWorkspaceConfig(workspaceId: string, patch: Partial<WorkspaceConfig>): Promise<WorkspaceDetail> {
  return apiRequest<WorkspaceDetail>(`/v1/workspaces/${workspaceId}/config`, {
    method: "PATCH",
    body: configPatchBody(patch),
  }).then(normalizeWorkspaceDetail);
}

export function triggerPromotion(workspaceId: string): Promise<PromotionReport> {
  return apiRequest<PromotionReport>(`/v1/workspaces/${workspaceId}/promote`, {
    method: "POST",
  });
}

export async function exportMemories(workspaceId: string): Promise<Blob> {
  const response = await fetch(`/v1/workspaces/${workspaceId}/export`, {
    headers: requestHeaders(),
  });
  const payload = response.ok ? null : await parseResponse(response);

  if (!response.ok) {
    throw new ApiError(response.status, extractDetail(payload, response.statusText));
  }

  return response.blob();
}

function configPatchBody(patch: Partial<WorkspaceConfig>): { [key: string]: JsonValue } {
  const body: { [key: string]: JsonValue } = {};

  Object.entries(patch).forEach(([key, value]) => {
    if (value !== undefined) {
      body[key] = value;
    }
  });

  return body;
}

function normalizeWorkspaceDetail(workspace: WorkspaceDetail): WorkspaceDetail {
  const config = configRecord(workspace.config);
  const normalized: WorkspaceDetail = { ...workspace };
  const decayHalfLifeDays = numberConfig(config.decay_half_life_days);
  const pruningThreshold = numberConfig(config.pruning_threshold);
  const llmProvider = stringConfig(config.llm_provider);
  const llmModel = stringConfig(config.llm_model);
  const embeddingProvider = stringConfig(config.embedding_provider);
  const embeddingModel = stringConfig(config.embedding_model);

  if (decayHalfLifeDays !== undefined) {
    normalized.decay_half_life_days = decayHalfLifeDays;
  }
  if (pruningThreshold !== undefined) {
    normalized.pruning_threshold = pruningThreshold;
  }
  if (llmProvider !== undefined) {
    normalized.llm_provider = llmProvider;
  }
  if (llmModel !== undefined) {
    normalized.llm_model = llmModel;
  }
  if (embeddingProvider !== undefined) {
    normalized.embedding_provider = embeddingProvider;
  }
  if (embeddingModel !== undefined) {
    normalized.embedding_model = embeddingModel;
  }

  return normalized;
}

function configRecord(config: WorkspaceDetail["config"]): Record<string, JsonValue | undefined> {
  if (config && typeof config === "object" && !Array.isArray(config)) {
    return config as Record<string, JsonValue | undefined>;
  }

  return {};
}

function numberConfig(value: JsonValue | undefined): number | undefined {
  return typeof value === "number" ? value : undefined;
}

function stringConfig(value: JsonValue | undefined): string | undefined {
  return typeof value === "string" ? value : undefined;
}
