import { apiRequest, parseResponse, queryString } from "./client";
import type {
  ListMemoryResponse,
  MemoryType,
  MemoryTypeFilter,
  MemoryUnit,
  ReadinessResponse,
  SearchRequest,
  SearchResponse,
  SortDirection,
  SortField,
  UpdateMemoryRequest,
} from "./types";

export type MemoryListParams = {
  limit?: number;
  offset?: number;
  memoryType?: MemoryTypeFilter;
  pinned?: boolean;
  minImportance?: number;
  sort?: SortField;
  direction?: SortDirection;
};

export type SearchCriteria = {
  query: string;
  memoryType: MemoryTypeFilter;
  pinned: boolean;
  minImportance: number;
  tags: string[];
  limit: number;
  offset: number;
};

export async function getReadiness(): Promise<ReadinessResponse> {
  const response = await fetch("/health/ready");
  const payload = await parseResponse(response);
  const base = isReadinessPayload(payload) ? payload : { status: response.ok ? "ok" : "unavailable" };

  return {
    ...base,
    httpStatus: response.status,
  };
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
    sort: params.sort,
    direction: params.direction,
  });

  return apiRequest<ListMemoryResponse>(`/v1/memory${search}`);
}

export function getMemory(workspaceId: string, id: string): Promise<MemoryUnit> {
  return apiRequest<MemoryUnit>(`/v1/memory/${id}${queryString({ workspace_id: workspaceId })}`);
}

export function patchMemory(workspaceId: string, id: string, patch: UpdateMemoryRequest): Promise<MemoryUnit> {
  return apiRequest<MemoryUnit>(`/v1/memory/${id}${queryString({ workspace_id: workspaceId })}`, {
    method: "PATCH",
    body: patch,
  });
}

export function searchMemory(request: SearchRequest): Promise<SearchResponse> {
  return apiRequest<SearchResponse>("/v1/memory/search", {
    method: "POST",
    body: request,
  });
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

  const request: SearchRequest = {
    query,
    workspace_id: workspaceId,
    mode: "hybrid",
    limit: criteria.limit,
    offset: criteria.offset,
  };

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

  const raw = field === "created_at" ? memory.created_at : memory.updated_at;
  const timestamp = Date.parse(raw);
  return Number.isNaN(timestamp) ? 0 : timestamp;
}

function isReadinessPayload(value: unknown): value is Omit<ReadinessResponse, "httpStatus"> {
  return typeof value === "object" && value !== null && "status" in value;
}

export const memoryTypeValues: MemoryType[] = ["episodic", "semantic"];
