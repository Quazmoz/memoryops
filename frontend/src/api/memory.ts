import { apiRequest, apiUrl, isAbortError, parseResponse, queryString } from "./client";
import type {
  FeedbackResponse,
  ListMemoryResponse,
  JsonValue,
  MemoryType,
  MemoryTypeFilter,
  MemoryUnit,
  ProvenanceGraph,
  ReadinessResponse,
  RetrieveRequest,
  RetrieveResponse,
  RetrievalTrace,
  SearchRequest,
  SearchResponse,
  ScopeFilter,
  SortDirection,
  SortField,
  SubmitFeedbackRequest,
  UpdateMemoryRequest,
} from "./types";

export type MemoryListParams = {
  limit?: number;
  offset?: number;
  memoryType?: MemoryTypeFilter;
  pinned?: boolean;
  minImportance?: number;
  agentId?: string;
  userId?: string;
  repo?: string;
  source?: string;
  sort?: SortField;
  direction?: SortDirection;
  asOf?: string;
};

export type SearchCriteria = {
  query: string;
  memoryType: MemoryTypeFilter;
  pinned: boolean;
  minImportance: number;
  tags: string[];
  agentId?: string;
  userId?: string;
  repo?: string;
  asOf?: string;
  includeWorkspacePool?: boolean;
  limit: number;
  offset: number;
};

const READINESS_TIMEOUT_MS = 5_000;

export async function getReadiness(): Promise<ReadinessResponse> {
  const controller = new AbortController();
  const timeoutId = window.setTimeout(() => controller.abort(), READINESS_TIMEOUT_MS);

  try {
    const response = await fetch(apiUrl("/health/ready"), { signal: controller.signal });
    const payload = await parseResponse(response);
    const base = isReadinessPayload(payload) ? payload : { status: response.ok ? "ok" : "unavailable" };

    return {
      ...base,
      httpStatus: response.status,
    };
  } catch (error) {
    if (isAbortError(error)) {
      return {
        status: "unavailable",
        httpStatus: 408,
      };
    }

    throw error;
  } finally {
    window.clearTimeout(timeoutId);
  }
}

export function listMemory(workspaceId: string, params: MemoryListParams): Promise<ListMemoryResponse> {
  const memoryType = params.memoryType === "all" ? undefined : params.memoryType;
  const search = queryString({
    workspace_id: workspaceId,
    limit: params.limit,
    offset: params.offset,
    memory_type: memoryType,
    pinned: params.pinned,
    min_importance: params.minImportance && params.minImportance > 0 ? params.minImportance : undefined,
    agent_id: optionalText(params.agentId),
    user_id: optionalText(params.userId),
    repo: optionalText(params.repo),
    source: optionalText(params.source),
    sort: params.sort,
    direction: params.direction,
    as_of: params.asOf,
  });

  return apiRequest<ListMemoryResponse>(`/v1/memory${search}`);
}

export function getMemory(workspaceId: string, id: string): Promise<MemoryUnit> {
  return apiRequest<MemoryUnit>(`/v1/memory/${id}${queryString({ workspace_id: workspaceId })}`);
}

export function getMemoryProvenance(workspaceId: string, id: string): Promise<ProvenanceGraph> {
  return apiRequest<ProvenanceGraph>(`/v1/memory/${id}/provenance${queryString({ workspace_id: workspaceId })}`);
}

export function submitFeedback(
  workspaceId: string,
  memoryId: string,
  request: SubmitFeedbackRequest,
): Promise<MemoryUnit> {
  return apiRequest<MemoryUnit>(`/v1/memory/${memoryId}/feedback${queryString({ workspace_id: workspaceId })}`, {
    method: "POST",
    body: request,
  });
}

export function getMemoryFeedback(
  workspaceId: string,
  memoryId: string,
  params: { limit?: number; offset?: number } = {},
): Promise<FeedbackResponse> {
  return apiRequest<FeedbackResponse>(
    `/v1/memory/${memoryId}/feedback${queryString({
      workspace_id: workspaceId,
      limit: params.limit,
      offset: params.offset,
    })}`,
  );
}

export function patchMemory(workspaceId: string, id: string, patch: UpdateMemoryRequest): Promise<MemoryUnit> {
  return apiRequest<MemoryUnit>(`/v1/memory/${id}${queryString({ workspace_id: workspaceId })}`, {
    method: "PATCH",
    body: patch,
  });
}

export function publishMemory(workspaceId: string, id: string): Promise<MemoryUnit> {
  return apiRequest<MemoryUnit>(`/v1/memory/${id}/publish${queryString({ workspace_id: workspaceId })}`, {
    method: "POST",
  });
}

export function searchMemory(request: SearchRequest): Promise<SearchResponse> {
  return apiRequest<SearchResponse>("/v1/memory/search", {
    method: "POST",
    body: request,
  });
}

export function postRetrieve(workspaceId: string, request: RetrieveRequest): Promise<RetrieveResponse> {
  return apiRequest<RetrieveResponse>("/v1/retrieve", {
    method: "POST",
    body: retrieveRequestBody(workspaceId, request),
  });
}

export function retrieveMemory(request: RetrieveRequest & { workspace_id: string }): Promise<RetrieveResponse> {
  const { workspace_id: workspaceId, ...retrieveRequest } = request;
  return postRetrieve(workspaceId, retrieveRequest);
}

export function getRetrievalTrace(workspaceId: string, queryId: string): Promise<RetrievalTrace> {
  return apiRequest<RetrievalTrace>(`/v1/retrieve/trace/${queryId}${queryString({ workspace_id: workspaceId })}`);
}

function retrieveRequestBody(workspaceId: string, request: RetrieveRequest): Record<string, JsonValue> {
  const body: Record<string, JsonValue> = {
    query: request.query,
    workspace_id: workspaceId,
    mode: request.mode ?? request.search_mode ?? "hybrid",
  };

  if (request.limit !== undefined) {
    body.limit = request.limit;
  }

  if (request.token_budget !== undefined) {
    body.token_budget = request.token_budget;
  }

  if (request.include_trace !== undefined) {
    body.include_trace = request.include_trace;
  }

  if (request.as_of !== undefined) {
    body.as_of = request.as_of;
  }

  if (request.include_workspace_pool !== undefined) {
    body.include_workspace_pool = request.include_workspace_pool;
  }

  if (request.scope !== undefined) {
    body.scope = request.scope as JsonValue;
  }

  const scope = scopeFilter(request.agent_id, request.user_id, request.repo);
  if (scope.agent_id !== undefined) {
    body.agent_id = scope.agent_id;
  }
  if (scope.user_id !== undefined) {
    body.user_id = scope.user_id;
  }
  if (scope.repo !== undefined) {
    body.repo = scope.repo;
  }

  return body;
}

export function buildSearchRequest(workspaceId: string, criteria: SearchCriteria): SearchRequest {
  const filters: SearchRequest["filters"] = {};
  const query = criteria.query.trim();

  if (criteria.memoryType !== "all") {
    filters.memory_type = criteria.memoryType;
  }

  if (criteria.pinned) {
    filters.pinned = true;
  }

  if (criteria.minImportance > 0) {
    filters.min_importance = criteria.minImportance;
  }

  if (criteria.tags.length > 0) {
    filters.tags = criteria.tags;
  }

  const scope = scopeFilter(criteria.agentId, criteria.userId, criteria.repo);
  const request: SearchRequest = {
    query,
    workspace_id: workspaceId,
    mode: "hybrid",
    limit: criteria.limit,
    offset: criteria.offset,
  };

  if (scope.agent_id !== undefined) {
    request.agent_id = scope.agent_id;
  }
  if (scope.user_id !== undefined) {
    request.user_id = scope.user_id;
  }
  if (scope.repo !== undefined) {
    request.repo = scope.repo;
  }

  if (criteria.memoryType !== "all") {
    request.memory_types = [criteria.memoryType];
  }

  if (criteria.asOf) {
    request.as_of = criteria.asOf;
  }

  if (criteria.includeWorkspacePool) {
    request.include_workspace_pool = true;
  }

  if (Object.keys(filters).length > 0) {
    request.filters = filters;
  }

  return request;
}

export function sortMemoryUnits(items: MemoryUnit[], field: SortField, direction: SortDirection): MemoryUnit[] {
  const multiplier = direction === "asc" ? 1 : -1;

  return [...items].sort((left, right) => {
    const leftValue = sortableValue(left, field);
    const rightValue = sortableValue(right, field);

    if (leftValue < rightValue) {
      return -1 * multiplier;
    }

    if (leftValue > rightValue) {
      return 1 * multiplier;
    }

    return 0;
  });
}

function sortableValue(memory: MemoryUnit, field: SortField): number {
  if (field === "importance_score") {
    return memory.importance_score;
  }

  if (field === "decay_score") {
    return memory.decay_score;
  }

  if (field === "relevance_score") {
    return memory.relevance_score ?? 0.5;
  }

  const raw = field === "created_at" ? memory.created_at : memory.updated_at;
  const timestamp = Date.parse(raw);
  return Number.isNaN(timestamp) ? 0 : timestamp;
}

function scopeFilter(agentId: string | undefined, userId: string | undefined, repo: string | undefined): ScopeFilter {
  const filter: ScopeFilter = {};
  const normalizedAgentId = optionalText(agentId);
  const normalizedUserId = optionalText(userId);
  const normalizedRepo = optionalText(repo);

  if (normalizedAgentId !== undefined) {
    filter.agent_id = normalizedAgentId;
  }
  if (normalizedUserId !== undefined) {
    filter.user_id = normalizedUserId;
  }
  if (normalizedRepo !== undefined) {
    filter.repo = normalizedRepo;
  }

  return filter;
}

function optionalText(value: string | undefined): string | undefined {
  const trimmed = value?.trim() ?? "";
  return trimmed.length > 0 ? trimmed : undefined;
}

function isReadinessPayload(value: unknown): value is Omit<ReadinessResponse, "httpStatus"> {
  return typeof value === "object" && value !== null && "status" in value;
}

export const memoryTypeValues: MemoryType[] = ["episodic", "semantic"];
