import { MemoryOpsConfig } from "./config";

const DEFAULT_REQUEST_TIMEOUT_MS = 15_000;
const SLOW_REQUEST_TIMEOUT_MS = 30_000;
const SLOW_PATHS = ["/v1/retrieve", "/v1/memory/search"];

export type SearchMode = "hybrid" | "keyword" | "vector";

export interface MemoryUnit {
  id?: string;
  workspace_id?: string;
  scope?: unknown;
  memory_type?: string;
  scope_visibility?: string;
  content?: string;
  importance_score?: number;
  decay_score?: number;
  relevance_score?: number;
  pinned?: boolean;
  tags?: string[];
  token_count?: number | null;
  source_events?: string[];
  source_episode_ids?: string[];
  corroboration_count?: number;
  promoted_at?: string | null;
  created_at?: string;
  updated_at?: string;
  [key: string]: unknown;
}

export interface MemorySearchResult extends MemoryUnit {
  score?: number;
  rank?: number;
  source?: string;
}

export interface MemoryListResponse {
  items: MemoryUnit[];
  total: number;
  limit: number;
  offset: number;
}

export interface MemoryListOptions {
  limit?: number;
  offset?: number;
  repo?: string;
  pinned?: boolean;
  memoryType?: "episodic" | "semantic";
  sort?: "importance_score" | "decay_score" | "relevance_score" | "updated_at" | "created_at";
  direction?: "asc" | "desc";
}

export interface SearchOptions {
  mode?: SearchMode;
  repo?: string;
  includeWorkspacePool?: boolean;
}

export interface RetrieveOptions {
  mode?: SearchMode;
  repo?: string;
  includeTrace?: boolean;
  includeWorkspacePool?: boolean;
}

export interface RetrievalMemory extends MemoryUnit {
  [key: string]: unknown;
}

export interface RetrievalResult {
  query_id?: string;
  memories?: RetrievalMemory[];
  packed_context?: string;
  context?: string;
  [key: string]: unknown;
}

export interface ObservationAccepted {
  id: string;
  status: "queued" | string;
}

export class MemoryOpsClient {
  constructor(private readonly config: MemoryOpsConfig) {}

  async health(): Promise<unknown> {
    return this.request("/health/ready", { method: "GET", authenticated: false });
  }

  async getWorkspace(): Promise<unknown> {
    return this.request(`/v1/workspaces/${encodeURIComponent(this.config.workspaceId)}`, {
      method: "GET",
      authenticated: true,
    });
  }

  async listMemory(options: MemoryListOptions = {}): Promise<MemoryListResponse> {
    const response = await this.request(`/v1/memory${queryString({
      workspace_id: this.config.workspaceId,
      limit: options.limit,
      offset: options.offset,
      repo: options.repo,
      pinned: options.pinned,
      memory_type: options.memoryType,
      sort: options.sort,
      direction: options.direction,
    })}`, {
      method: "GET",
      authenticated: true,
    });

    if (!isRecord(response)) {
      return { items: [], total: 0, limit: options.limit ?? 0, offset: options.offset ?? 0 };
    }

    const items = Array.isArray(response.items) ? response.items.filter(isRecord).map(normalizeMemoryUnit) : [];
    return {
      items,
      total: numberOrDefault(response.total, items.length),
      limit: numberOrDefault(response.limit, options.limit ?? items.length),
      offset: numberOrDefault(response.offset, options.offset ?? 0),
    };
  }

  async searchMemory(query: string, topK: number, options: SearchOptions = {}): Promise<MemorySearchResult[]> {
    const response = await this.request("/v1/memory/search", {
      method: "POST",
      authenticated: true,
      body: {
        query,
        workspace_id: this.config.workspaceId,
        mode: options.mode ?? "hybrid",
        limit: topK,
        top_k: topK,
        repo: options.repo,
        include_workspace_pool: options.includeWorkspacePool,
      },
    });

    return normalizeSearchResults(response);
  }

  async retrieve(query: string, tokenBudget: number, options: RetrieveOptions = {}): Promise<RetrievalResult> {
    const response = await this.request("/v1/retrieve", {
      method: "POST",
      authenticated: true,
      body: {
        query,
        workspace_id: this.config.workspaceId,
        token_budget: tokenBudget,
        mode: options.mode ?? "hybrid",
        repo: options.repo,
        include_trace: options.includeTrace,
        include_workspace_pool: options.includeWorkspacePool,
      },
    });

    return isRecord(response) ? (response as RetrievalResult) : {};
  }

  async updateMemory(id: string, patch: { pinned?: boolean; tags?: string[]; importance_score?: number }): Promise<MemoryUnit> {
    const response = await this.request(`/v1/memory/${encodeURIComponent(id)}${queryString({
      workspace_id: this.config.workspaceId,
    })}`, {
      method: "PATCH",
      authenticated: true,
      body: patch,
    });

    if (!isRecord(response)) {
      throw new Error("MemoryOps returned an unexpected memory response.");
    }

    return normalizeMemoryUnit(response);
  }

  async deleteMemory(id: string): Promise<MemoryUnit | undefined> {
    const response = await this.request(`/v1/memory/${encodeURIComponent(id)}${queryString({
      workspace_id: this.config.workspaceId,
    })}`, {
      method: "DELETE",
      authenticated: true,
    });

    return isRecord(response) ? normalizeMemoryUnit(response) : undefined;
  }

  async saveObservation(input: {
    content: string;
    agentId: string;
    repo?: string;
    tags?: string[];
    sourceRef?: string;
  }): Promise<ObservationAccepted> {
    const response = await this.request("/v1/ingest/observation", {
      method: "POST",
      authenticated: true,
      body: {
        workspace_id: this.config.workspaceId,
        content: input.content,
        agent_id: input.agentId,
        repo: input.repo,
        tags: input.tags ?? ["vscode"],
        source_ref: input.sourceRef,
      },
    });

    if (!isRecord(response) || typeof response.id !== "string") {
      throw new Error("MemoryOps returned an unexpected observation response.");
    }

    return response as unknown as ObservationAccepted;
  }

  private async request(path: string, options: {
    method: "GET" | "POST" | "PATCH" | "DELETE";
    authenticated: boolean;
    body?: unknown;
  }): Promise<unknown> {
    const timeoutMs = requestTimeoutMs(path);
    const controller = new AbortController();
    const timeout = setTimeout(() => controller.abort(), timeoutMs);
    const headers: Record<string, string> = {
      Accept: "application/json",
    };

    if (options.body !== undefined) {
      headers["Content-Type"] = "application/json";
    }

    if (options.authenticated) {
      headers["X-API-Key"] = this.config.apiKey;
    }

    try {
      const response = await fetch(`${this.config.apiUrl}${path}`, {
        method: options.method,
        headers,
        body: options.body === undefined ? undefined : JSON.stringify(options.body),
        signal: controller.signal,
      });

      const text = await response.text();
      const payload = text.length > 0 ? parseJson(text) : undefined;

      if (!response.ok) {
        const message = extractErrorMessage(payload) ?? response.statusText;
        throw new Error(`MemoryOps ${response.status}: ${message}`);
      }

      return payload;
    } catch (error) {
      if (isAbortError(error)) {
        throw new Error(`MemoryOps request timed out after ${Math.round(timeoutMs / 1000)}s.`);
      }
      throw error;
    } finally {
      clearTimeout(timeout);
    }
  }
}

function requestTimeoutMs(path: string): number {
  return SLOW_PATHS.some((slowPath) => path.startsWith(slowPath))
    ? SLOW_REQUEST_TIMEOUT_MS
    : DEFAULT_REQUEST_TIMEOUT_MS;
}

function parseJson(text: string): unknown {
  try {
    return JSON.parse(text);
  } catch {
    return text;
  }
}

function normalizeSearchResults(response: unknown): MemorySearchResult[] {
  if (Array.isArray(response)) {
    return response.filter(isRecord).map(normalizeSearchItem);
  }

  if (!isRecord(response)) {
    return [];
  }

  const results = response.results ?? response.items ?? response.memories;
  return Array.isArray(results) ? results.filter(isRecord).map(normalizeSearchItem) : [];
}

function normalizeSearchItem(item: Record<string, unknown>): MemorySearchResult {
  const memory = isRecord(item.memory) ? normalizeMemoryUnit(item.memory) : normalizeMemoryUnit(item);
  return {
    ...memory,
    score: numberOrUndefined(item.score),
    rank: numberOrUndefined(item.rank),
  };
}

function normalizeMemoryUnit(value: Record<string, unknown>): MemoryUnit {
  return {
    ...value,
    id: stringOrUndefined(value.id),
    workspace_id: stringOrUndefined(value.workspace_id),
    memory_type: stringOrUndefined(value.memory_type),
    scope_visibility: stringOrUndefined(value.scope_visibility),
    content: stringOrUndefined(value.content),
    importance_score: numberOrUndefined(value.importance_score),
    decay_score: numberOrUndefined(value.decay_score),
    relevance_score: numberOrUndefined(value.relevance_score),
    pinned: booleanOrUndefined(value.pinned),
    tags: stringArrayOrUndefined(value.tags),
    token_count: numberOrNullOrUndefined(value.token_count),
    source_events: stringArrayOrUndefined(value.source_events),
    source_episode_ids: stringArrayOrUndefined(value.source_episode_ids),
    corroboration_count: numberOrUndefined(value.corroboration_count),
    promoted_at: stringOrNullOrUndefined(value.promoted_at),
    created_at: stringOrUndefined(value.created_at),
    updated_at: stringOrUndefined(value.updated_at),
  };
}

function extractErrorMessage(payload: unknown): string | undefined {
  if (!isRecord(payload)) {
    return undefined;
  }

  const candidates = [payload.message, payload.error, payload.detail];
  for (const candidate of candidates) {
    if (typeof candidate === "string" && candidate.trim().length > 0) {
      return candidate;
    }
  }

  return undefined;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null;
}

function queryString(params: Record<string, unknown>): string {
  const search = new URLSearchParams();
  for (const [key, value] of Object.entries(params)) {
    if (value === undefined || value === null || value === "") {
      continue;
    }
    search.set(key, String(value));
  }

  const value = search.toString();
  return value ? `?${value}` : "";
}

function stringOrUndefined(value: unknown): string | undefined {
  return typeof value === "string" ? value : undefined;
}

function stringOrNullOrUndefined(value: unknown): string | null | undefined {
  return value === null || typeof value === "string" ? value : undefined;
}

function numberOrUndefined(value: unknown): number | undefined {
  return typeof value === "number" && Number.isFinite(value) ? value : undefined;
}

function numberOrNullOrUndefined(value: unknown): number | null | undefined {
  return value === null || (typeof value === "number" && Number.isFinite(value)) ? value : undefined;
}

function numberOrDefault(value: unknown, fallback: number): number {
  return typeof value === "number" && Number.isFinite(value) ? value : fallback;
}

function booleanOrUndefined(value: unknown): boolean | undefined {
  return typeof value === "boolean" ? value : undefined;
}

function stringArrayOrUndefined(value: unknown): string[] | undefined {
  return Array.isArray(value) ? value.filter((item): item is string => typeof item === "string") : undefined;
}

function isAbortError(error: unknown): boolean {
  return isRecord(error) && error.name === "AbortError";
}
