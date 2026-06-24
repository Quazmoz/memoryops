import { ApiError, apiRequest, apiUrl, extractDetail, parseResponse, queryString, requestHeaders } from "./client";
import { apiContractRequest, operationMethod, resolveOperationPath } from "./generated/contract";
import type {
  ApiKeySummary,
  CreatedApiKey,
  CreateApiKeyResponse,
  CreateWorkspaceResponse,
  ForgetUserDataResponse,
  ImportMemoriesResponse,
  JsonValue,
  PromotionReport,
  StatsHistory,
  TagsResponse,
  WorkspaceConfig,
  WorkspaceDetail,
  WorkspaceSummary,
  WorkspaceStats,
} from "./types";

export type { ApiKeySummary } from "./types";

export async function createWorkspace(name: string, adminToken: string): Promise<WorkspaceSummary> {
  const trimmedName = name.trim();
  if (trimmedName.length === 0) {
    throw new Error("Workspace name is required");
  }

  const response = await apiContractRequest<CreateWorkspaceResponse>("createWorkspace", {
    auth: false,
    headers: { "x-admin-token": adminToken.trim() },
    body: { name: trimmedName },
  });
  const id = response.id ?? response.workspace_id;

  if (!id) {
    throw new Error("Workspace response did not include an id");
  }

  const result: WorkspaceSummary = {
    id,
    name: response.name ?? trimmedName,
  };

  if (response.api_key) {
    result.api_key = response.api_key;
  }

  return result;
}

export async function getDefaultWorkspace(): Promise<WorkspaceSummary> {
  const response = await apiRequest<CreateWorkspaceResponse>("/v1/default-workspace", {
    auth: false,
  });
  const id = response.id ?? response.workspace_id;

  if (!id || !response.api_key) {
    throw new Error("Default workspace response did not include credentials");
  }

  return {
    id,
    name: response.name ?? "Capsule Corp Memory Lab",
    api_key: response.api_key,
  };
}

export async function loginAdmin(password: string): Promise<boolean> {
  const trimmedPassword = password.trim();
  if (trimmedPassword.length === 0) {
    throw new Error("Root password is required");
  }

  const response = await apiRequest<{ ok: boolean }>("/v1/admin/session", {
    method: "POST",
    auth: false,
    body: { password: trimmedPassword },
  });

  return response.ok;
}

export async function createApiKey(workspaceId: string, name: string): Promise<CreatedApiKey> {
  const trimmedName = name.trim();
  if (trimmedName.length === 0) {
    throw new Error("API key name is required");
  }

  const response = await apiContractRequest<CreateApiKeyResponse>("createApiKey", {
    path: resolveOperationPath("createApiKey", { id: workspaceId }),
    body: { name: trimmedName },
  });
  const plaintextKey = response.plaintext_key ?? response.key;

  if (!plaintextKey) {
    throw new Error("API key response did not include a plaintext key");
  }

  return { plaintext_key: plaintextKey };
}

export function listApiKeys(workspaceId: string, includeRevoked = false): Promise<ApiKeySummary[]> {
  return apiContractRequest<ApiKeySummary[]>("listApiKeys", {
    path: `${resolveOperationPath("listApiKeys", { id: workspaceId })}${queryString({ include_revoked: includeRevoked })}`,
  });
}

export function revokeApiKey(workspaceId: string, keyId: string): Promise<ApiKeySummary> {
  return apiContractRequest<ApiKeySummary>("revokeApiKey", {
    path: resolveOperationPath("revokeApiKey", { id: workspaceId, key_id: keyId }),
  });
}

export function getWorkspace(workspaceId: string): Promise<WorkspaceDetail> {
  return apiContractRequest<WorkspaceDetail>("getWorkspace", {
    path: resolveOperationPath("getWorkspace", { id: workspaceId }),
  }).then(normalizeWorkspaceDetail);
}

export function getWorkspaceStats(workspaceId: string): Promise<WorkspaceStats> {
  return apiContractRequest<WorkspaceStats>("getWorkspaceStats", {
    path: resolveOperationPath("getWorkspaceStats", { id: workspaceId }),
  });
}

export function getWorkspaceStatsHistory(workspaceId: string, days = 30): Promise<StatsHistory> {
  return apiContractRequest<StatsHistory>("getWorkspaceStatsHistory", {
    path: `${resolveOperationPath("getWorkspaceStatsHistory", { id: workspaceId })}${queryString({ days })}`,
  });
}

export function listWorkspaceTags(workspaceId: string, limit = 50, cursor?: string): Promise<TagsResponse> {
  return apiContractRequest<TagsResponse>("listWorkspaceTags", {
    path: `${resolveOperationPath("listWorkspaceTags", { id: workspaceId })}${queryString({ limit, cursor })}`,
  });
}

export function updateWorkspaceConfig(workspaceId: string, patch: Partial<WorkspaceConfig>): Promise<WorkspaceDetail> {
  return apiContractRequest<WorkspaceDetail>("updateWorkspaceConfig", {
    path: resolveOperationPath("updateWorkspaceConfig", { id: workspaceId }),
    body: configPatchBody(patch),
  }).then(normalizeWorkspaceDetail);
}

export function triggerPromotion(workspaceId: string): Promise<PromotionReport> {
  return apiContractRequest<PromotionReport>("promoteWorkspace", {
    path: resolveOperationPath("promoteWorkspace", { id: workspaceId }),
  });
}

export function forgetUserData(workspaceId: string, userId: string): Promise<ForgetUserDataResponse> {
  return apiContractRequest<ForgetUserDataResponse>("forgetUserData", {
    path: resolveOperationPath("forgetUserData", { workspace_id: workspaceId, user_id: userId }),
  });
}

export async function exportMemories(workspaceId: string): Promise<Blob> {
  const path = resolveOperationPath("exportWorkspaceMemory", { id: workspaceId });
  const response = await fetch(apiUrl(path), {
    method: operationMethod("exportWorkspaceMemory").toUpperCase(),
    headers: requestHeaders(),
  });
  const payload = response.ok ? null : await parseResponse(response);

  if (!response.ok) {
    throw new ApiError(response.status, extractDetail(payload, response.statusText));
  }

  return response.blob();
}

export async function importMemories(workspaceId: string, file: File): Promise<ImportMemoriesResponse> {
  const headers = requestHeaders({ headers: { "content-type": "application/x-ndjson" } });
  const path = resolveOperationPath("importWorkspaceMemory", { id: workspaceId });
  const response = await fetch(apiUrl(path), {
    method: operationMethod("importWorkspaceMemory").toUpperCase(),
    headers,
    body: file,
  });
  const payload = await parseResponse(response);

  if (!response.ok) {
    throw new ApiError(response.status, extractDetail(payload, response.statusText));
  }

  return payload as ImportMemoriesResponse;
}

export interface WorkspaceListItem {
  id: string;
  name: string;
  created_at: string;
}

export interface WorkspaceListResponse {
  workspaces: WorkspaceListItem[];
}

export async function listWorkspaces(apiKey: string): Promise<WorkspaceListItem[]> {
  const trimmedApiKey = apiKey.trim();
  if (trimmedApiKey.length === 0) {
    return [];
  }

  const headers = requestHeaders({}, false);
  headers.set("x-api-key", trimmedApiKey);
  const response = await fetch(apiUrl(resolveOperationPath("listWorkspaces", {})), {
    method: operationMethod("listWorkspaces").toUpperCase(),
    headers,
  });
  const payload = await parseResponse(response);

  if (!response.ok) {
    throw new ApiError(response.status, extractDetail(payload, response.statusText));
  }

  return normalizeWorkspaceList(payload);
}

export function normalizeWorkspaceList(payload: unknown): WorkspaceListItem[] {
  const listed = (payload as WorkspaceListResponse | null)?.workspaces;
  if (Array.isArray(listed)) {
    return listed.filter(
      (workspace): workspace is WorkspaceListItem =>
        typeof workspace?.id === "string" && typeof workspace?.name === "string",
    );
  }

  const single = payload as Partial<WorkspaceListItem> | null;
  if (single && typeof single.id === "string" && typeof single.name === "string") {
    return [
      {
        id: single.id,
        name: single.name,
        created_at: typeof single.created_at === "string" ? single.created_at : "",
      },
    ];
  }

  return [];
}

export type DeleteWorkspaceResponse = {
  deleted: boolean;
};

export async function deleteWorkspace(workspaceId: string): Promise<DeleteWorkspaceResponse> {
  const response = await apiContractRequest<DeleteWorkspaceResponse | null>("deleteWorkspace", {
    path: resolveOperationPath("deleteWorkspace", { id: workspaceId }),
  });

  return { deleted: response?.deleted ?? true };
}

export interface ReindexResponse {
  enqueued: number;
  next_cursor: string | null;
}

export function triggerReindex(workspaceId: string, force = false, after?: string): Promise<ReindexResponse> {
  return apiContractRequest<ReindexResponse>("reindexWorkspace", {
    path: `${resolveOperationPath("reindexWorkspace", { id: workspaceId })}${queryString({ force: force ? true : undefined, after })}`,
  });
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
  const llmBaseUrl = stringConfig(config.llm_base_url);
  const llmApiKeyEnv = stringConfig(config.llm_api_key_env);
  const embeddingProvider = stringConfig(config.embedding_provider);
  const embeddingModel = stringConfig(config.embedding_model);
  const subAgentPools = stringArrayConfig(config.sub_agent_pools);
  const retentionMaxAgeDays = numberConfig(config.retention_max_age_days);
  const skillVersionRetentionDays = numberConfig(config.skill_version_retention_days);
  const complianceHardPurge = booleanConfig(config.compliance_hard_purge);
  const complianceMode = booleanConfig(config.compliance_mode);
  const contradictionMode = stringConfig(config.contradiction_mode);
  const contradictionThreshold = numberConfig(config.contradiction_threshold);
  const contradictionCandidates = numberConfig(config.contradiction_candidates);

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
  if (llmBaseUrl !== undefined) {
    normalized.llm_base_url = llmBaseUrl;
  }
  if (llmApiKeyEnv !== undefined) {
    normalized.llm_api_key_env = llmApiKeyEnv;
  }
  if (embeddingProvider !== undefined) {
    normalized.embedding_provider = embeddingProvider;
  }
  if (embeddingModel !== undefined) {
    normalized.embedding_model = embeddingModel;
  }
  if (subAgentPools !== undefined) {
    normalized.sub_agent_pools = subAgentPools;
  }
  if (retentionMaxAgeDays !== undefined) {
    normalized.retention_max_age_days = retentionMaxAgeDays;
  }
  if (skillVersionRetentionDays !== undefined) {
    normalized.skill_version_retention_days = skillVersionRetentionDays;
  }
  if (complianceHardPurge !== undefined) {
    normalized.compliance_hard_purge = complianceHardPurge;
  }
  if (complianceMode !== undefined) {
    normalized.compliance_mode = complianceMode;
  }
  if (contradictionMode !== undefined) {
    normalized.contradiction_mode = contradictionMode;
  }
  if (contradictionThreshold !== undefined) {
    normalized.contradiction_threshold = contradictionThreshold;
  }
  if (contradictionCandidates !== undefined) {
    normalized.contradiction_candidates = contradictionCandidates;
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
  if (typeof value !== "string") {
    return undefined;
  }

  const trimmed = value.trim();
  return trimmed.length > 0 ? trimmed : undefined;
}

function booleanConfig(value: JsonValue | undefined): boolean | undefined {
  return typeof value === "boolean" ? value : undefined;
}

function stringArrayConfig(value: JsonValue | undefined): string[] | undefined {
  if (!Array.isArray(value) || value.some((item) => typeof item !== "string")) {
    return undefined;
  }

  const strings = value as string[];
  const normalized = strings.map((item) => item.trim()).filter((item) => item.length > 0);
  return Array.from(new Set(normalized));
}
