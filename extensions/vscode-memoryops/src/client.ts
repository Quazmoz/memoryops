import { MemoryOpsConfig } from "./config";

const DEFAULT_REQUEST_TIMEOUT_MS = 15_000;
const SLOW_REQUEST_TIMEOUT_MS = 30_000;
const SLOW_PATHS = ["/v1/retrieve", "/v1/memory/search"];

// Only GET/idempotent reads and explicitly safe POSTs are retried. Mutating
// writes (PATCH/DELETE, and POSTs that create/merge) are never auto-retried to
// avoid duplicate side effects.
const RETRYABLE_METHODS = new Set(["GET"]);
const RETRYABLE_STATUS = new Set([408, 425, 429, 500, 502, 503, 504]);

export type SearchMode = "hybrid" | "keyword" | "vector";

export interface MemoryUnit {
  id?: string;
  workspace_id?: string;
  scope?: unknown;
  memory_type?: string;
  scope_visibility?: string;
  content?: string;
  score?: number;
  rank?: number;
  importance_score?: number;
  decay_score?: number;
  relevance_score?: number;
  pinned?: boolean;
  tags?: string[];
  token_count?: number | null;
  source_events?: string[];
  source_episode_ids?: string[];
  corroboration_count?: number;
  version?: number;
  promoted_at?: string | null;
  deleted_at?: string | null;
  created_at?: string;
  updated_at?: string;
  [key: string]: unknown;
}

export interface MemorySearchResult extends MemoryUnit {
  source?: string;
}

export interface MemoryVersion {
  id?: string;
  memory_id?: string;
  workspace_id?: string;
  version?: number;
  content?: string;
  importance_score?: number;
  tags?: string[];
  edited_by?: string;
  created_at?: string;
  [key: string]: unknown;
}

export interface ProvenanceNode {
  id?: string;
  node_type?: string;
  title?: string;
  subtitle?: string | null;
  timestamp?: string | null;
  metadata?: Record<string, unknown>;
  [key: string]: unknown;
}

export interface ProvenanceEdge {
  from?: string;
  to?: string;
  edge_type?: string;
  [key: string]: unknown;
}

export interface ProvenanceGraph {
  root_id?: string;
  nodes: ProvenanceNode[];
  edges: ProvenanceEdge[];
  [key: string]: unknown;
}

export interface FeedbackEntry {
  id?: string;
  memory_id?: string;
  query_id?: string;
  agent_id?: string | null;
  user_id?: string | null;
  rating: number;
  comment?: string | null;
  occurred_at?: string;
  [key: string]: unknown;
}

export interface FeedbackResponse {
  items: FeedbackEntry[];
  total: number;
  memory_id?: string;
  avg_rating?: number;
  relevance_score?: number;
  [key: string]: unknown;
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
  // Filter to memories derived from a specific source file (line anchors ignored).
  sourceRef?: string;
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

export interface FeedbackListOptions {
  limit?: number;
  offset?: number;
}

export interface MemoryUpdatePatch {
  content?: string;
  pinned?: boolean;
  tags?: string[];
  importance_score?: number;
}

export type BulkMemoryAction = "pin" | "unpin" | "delete";
export interface BulkMemoryResponse {
  affected: number;
  affected_ids?: string[];
  requested?: number;
  action?: BulkMemoryAction;
}

export interface RetrievalMemory extends MemoryUnit {
  [key: string]: unknown;
}

export interface RetrievalResult {
  query_id?: string;
  memories?: RetrievalMemory[];
  packed_context?: string;
  context?: string;
  total_tokens?: number;
  [key: string]: unknown;
}

export interface ObservationAccepted {
  id: string;
  status: "queued" | string;
}

interface HttpError extends Error {
  status?: number;
  transient?: boolean;
}

export interface Skill {
  id: string;
  workspace_id: string;
  name: string;
  description: string;
  endpoint_url: string;
  http_method: string;
  input_schema: unknown;
  output_schema: unknown;
  auth_header: string | null;
  enabled: boolean;
  version: number;
  scope_visibility: "private" | "workspace" | "published";
  created_at?: string;
  updated_at?: string;
}

export interface SkillVersion {
  id: string;
  skill_id: string;
  workspace_id: string;
  name: string;
  version: number;
  description: string;
  endpoint_url: string;
  http_method: string;
  input_schema: unknown;
  output_schema: unknown;
  auth_header: string | null;
  enabled: boolean;
  scope_visibility: "private" | "workspace" | "published";
  change_note: string | null;
  created_by: string | null;
  created_at?: string;
}

export interface ToolInvocation {
  id: number;
  tool_id: string;
  workspace_id: string;
  tool_name: string;
  tool_version: number;
  actor: string;
  source: string;
  status_code: number;
  latency_ms: number;
  error: string | null;
  occurred_at: string;
  [key: string]: unknown;
}

export interface SkillCreateInput {
  name: string;
  description: string;
  endpoint_url: string;
  http_method?: string;
  input_schema?: unknown;
  output_schema?: unknown;
  auth_header?: string;
  auth_secret?: string;
  enabled?: boolean;
  change_note?: string;
  scope_visibility?: "private" | "workspace" | "published";
}

export interface SkillUpdateInput {
  description?: string;
  endpoint_url?: string;
  http_method?: string;
  input_schema?: unknown;
  output_schema?: unknown;
  auth_header?: string;
  auth_secret?: string;
  enabled?: boolean;
  change_note?: string;
  scope_visibility?: "private" | "workspace" | "published";
}

export interface SkillTestResult {
  status: number;
  latency_ms: number;
  body: unknown;
}

export interface AgentSkill {
  name: string;
  filename: string;
  assistant: string;
  title: string;
  description: string;
  version: number;
}

export interface AgentSkillContent {
  name: string;
  filename: string;
  assistant: string;
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
  assistant: string;
  title: string;
  description: string;
  instructions: string;
  content: string;
  change_note: string | null;
  created_by: string | null;
  created_at: string;
}

export interface AgentSkillCreateInput {
  assistant: string;
  name: string;
  title: string;
  description: string;
  instructions: string;
  change_note?: string;
}

export interface AgentSkillUpdateInput {
  title: string;
  description: string;
  instructions: string;
  change_note?: string;
}

export interface ContradictionMemoryRef {
  id: string;
  content_preview: string;
  created_at: string;
}

export interface ContradictionItem {
  id: string;
  workspace_id: string;
  memory_a: ContradictionMemoryRef;
  memory_b: ContradictionMemoryRef;
  similarity: number;
  conflict_score: number;
  resolution: string;
  resolved_by: string | null;
  resolved_at: string | null;
  notes: string | null;
  kept_memory_id: string | null;
  discarded_memory_id: string | null;
  created_at: string;
}

export type ContradictionResolution = "accepted" | "dismissed" | "keep_a" | "keep_b";

export interface ContradictionListResponse {
  items: ContradictionItem[];
  next_cursor: string | null;
}

export class MemoryOpsClient {
  constructor(
    private readonly config: MemoryOpsConfig,
    private readonly log?: (message: string) => void,
  ) {}

  async health(): Promise<unknown> {
    return this.request("/health/ready", { method: "GET", authenticated: false, idempotent: true });
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
      source_ref: options.sourceRef,
    })}`, {
      method: "GET",
      authenticated: true,
      idempotent: true,
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
      idempotent: true,
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
      idempotent: true,
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

  async updateMemory(id: string, patch: MemoryUpdatePatch): Promise<MemoryUnit> {
    const response = await this.request(this.memoryPath(id), {
      method: "PATCH",
      authenticated: true,
      body: patch,
    });

    return expectMemoryUnit(response, "MemoryOps returned an unexpected memory response.");
  }

  async deleteMemory(id: string): Promise<MemoryUnit | undefined> {
    const response = await this.request(this.memoryPath(id), {
      method: "DELETE",
      authenticated: true,
    });

    return isRecord(response) ? normalizeMemoryUnit(response) : undefined;
  }

  async promoteMemory(id: string): Promise<MemoryUnit> {
    const response = await this.request(this.memoryPath(id, "/promote"), {
      method: "POST",
      authenticated: true,
    });

    return expectMemoryUnit(response, "MemoryOps returned an unexpected promoted memory response.");
  }

  async publishMemory(id: string): Promise<MemoryUnit> {
    const response = await this.request(this.memoryPath(id, "/publish"), {
      method: "POST",
      authenticated: true,
    });

    return expectMemoryUnit(response, "MemoryOps returned an unexpected published memory response.");
  }

  async mergeMemory(sourceId: string, targetId: string): Promise<MemoryUnit> {
    const response = await this.request(`/v1/memory/merge${queryString({ workspace_id: this.config.workspaceId })}`, {
      method: "POST",
      authenticated: true,
      body: {
        source_id: sourceId,
        target_id: targetId,
      },
    });

    return expectMemoryUnit(response, "MemoryOps returned an unexpected merge response.");
  }

  async bulkMemory(ids: string[], action: BulkMemoryAction): Promise<BulkMemoryResponse> {
    const response = await this.request(`/v1/memory/bulk${queryString({ workspace_id: this.config.workspaceId })}`, {
      method: "POST",
      authenticated: true,
      body: {
        ids,
        action,
      },
    });

    if (!isRecord(response)) {
      return { affected: 0, affected_ids: [], requested: 0, action };
    }

    return {
      affected: numberOrDefault(response.affected, 0),
      affected_ids: stringArrayOrUndefined(response.affected_ids) ?? [],
      requested: numberOrDefault(response.requested, ids.length),
      action: (response.action as BulkMemoryAction) ?? action,
    };
  }

  async getMemoryHistory(id: string): Promise<MemoryVersion[]> {
    const response = await this.request(this.memoryPath(id, "/history"), {
      method: "GET",
      authenticated: true,
    });

    if (!isRecord(response)) {
      return [];
    }

    return Array.isArray(response.items)
      ? response.items.filter(isRecord).map(normalizeMemoryVersion)
      : [];
  }

  async getMemoryProvenance(id: string): Promise<ProvenanceGraph> {
    const response = await this.request(this.memoryPath(id, "/provenance"), {
      method: "GET",
      authenticated: true,
    });

    return normalizeProvenanceGraph(response);
  }

  async getMemoryFeedback(id: string, options: FeedbackListOptions = {}): Promise<FeedbackResponse> {
    const response = await this.request(this.memoryPath(id, "/feedback", {
      limit: options.limit,
      offset: options.offset,
    }), {
      method: "GET",
      authenticated: true,
    });

    return normalizeFeedbackResponse(response);
  }

  async submitMemoryFeedback(
    memoryId: string,
    input: {
      queryId: string;
      rating: number;
      agentId?: string | null;
      userId?: string | null;
      comment?: string | null;
    }
  ): Promise<unknown> {
    return this.request(this.memoryPath(memoryId, "/feedback"), {
      method: "POST",
      authenticated: true,
      body: {
        query_id: input.queryId,
        rating: input.rating,
        agent_id: input.agentId ?? undefined,
        user_id: input.userId ?? undefined,
        comment: input.comment ?? undefined,
      },
    });
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

  async listSkills(): Promise<Skill[]> {
    const response = await this.request(
      `/v1/workspaces/${encodeURIComponent(this.config.workspaceId)}/tools`,
      { method: "GET", authenticated: true, idempotent: true },
    );
    return Array.isArray(response) ? response.filter(isRecord).map(normalizeSkill) : [];
  }

  async createSkill(input: SkillCreateInput): Promise<Skill> {
    const response = await this.request(
      `/v1/workspaces/${encodeURIComponent(this.config.workspaceId)}/tools`,
      {
        method: "POST",
        authenticated: true,
        body: {
          name: input.name,
          description: input.description,
          endpoint_url: input.endpoint_url,
          http_method: input.http_method ?? "POST",
          input_schema: input.input_schema ?? {},
          output_schema: input.output_schema ?? {},
          auth_header: input.auth_header,
          auth_secret: input.auth_secret,
          enabled: input.enabled ?? true,
          change_note: input.change_note,
          scope_visibility: input.scope_visibility,
        },
      },
    );
    return expectSkill(response, "MemoryOps returned an unexpected skill response.");
  }

  async updateSkill(name: string, patch: SkillUpdateInput): Promise<Skill> {
    const response = await this.request(
      `/v1/workspaces/${encodeURIComponent(this.config.workspaceId)}/tools/${encodeURIComponent(name)}`,
      { method: "PATCH", authenticated: true, body: patch },
    );
    return expectSkill(response, "MemoryOps returned an unexpected skill response.");
  }

  async deleteSkill(name: string): Promise<void> {
    await this.request(
      `/v1/workspaces/${encodeURIComponent(this.config.workspaceId)}/tools/${encodeURIComponent(name)}`,
      { method: "DELETE", authenticated: true },
    );
  }

  async testSkill(name: string, body: unknown, version?: number): Promise<SkillTestResult> {
    const response = await this.request(
      `/v1/workspaces/${encodeURIComponent(this.config.workspaceId)}/tools/${encodeURIComponent(name)}/test`,
      { method: "POST", authenticated: true, idempotent: true, body: { body, version } },
    );
    if (!isRecord(response)) {
      return { status: 0, latency_ms: 0, body: response };
    }
    return {
      status: numberOrDefault(response.status, 0),
      latency_ms: numberOrDefault(response.latency_ms, 0),
      body: response.body,
    };
  }

  async listSkillVersions(name: string): Promise<SkillVersion[]> {
    const response = await this.request(
      `/v1/workspaces/${encodeURIComponent(this.config.workspaceId)}/tools/${encodeURIComponent(name)}/versions`,
      { method: "GET", authenticated: true, idempotent: true },
    );
    return Array.isArray(response) ? response.filter(isRecord).map(normalizeSkillVersion) : [];
  }

  async rollbackSkillVersion(name: string, version: number, changeNote?: string): Promise<Skill> {
    const response = await this.request(
      `/v1/workspaces/${encodeURIComponent(this.config.workspaceId)}/tools/${encodeURIComponent(name)}/versions/${version}/rollback`,
      {
        method: "POST",
        authenticated: true,
        body: changeNote ? { change_note: changeNote } : {},
      },
    );
    return expectSkill(response, "MemoryOps returned an unexpected rollback response.");
  }

  async invokeSkill(name: string, body: unknown, version?: number): Promise<SkillTestResult> {
    const response = await this.request(
      `/v1/workspaces/${encodeURIComponent(this.config.workspaceId)}/tools/${encodeURIComponent(name)}/invoke`,
      { method: "POST", authenticated: true, body: { body, version } },
    );
    if (!isRecord(response)) {
      return { status: 0, latency_ms: 0, body: response };
    }
    return {
      status: numberOrDefault(response.status, 0),
      latency_ms: numberOrDefault(response.latency_ms, 0),
      body: response.body,
    };
  }

  async listSkillInvocations(name: string, limit = 50): Promise<ToolInvocation[]> {
    const response = await this.request(
      `/v1/workspaces/${encodeURIComponent(this.config.workspaceId)}/tools/${encodeURIComponent(name)}/invocations?limit=${limit}`,
      { method: "GET", authenticated: true, idempotent: true },
    );
    return Array.isArray(response) ? response.filter(isRecord).map(normalizeToolInvocation) : [];
  }

  async listAgentSkills(): Promise<AgentSkill[]> {
    const response = await this.request("/v1/agent-skills", {
      method: "GET",
      authenticated: true,
      idempotent: true,
    });
    return Array.isArray(response) ? response.filter(isRecord).map(normalizeAgentSkill) : [];
  }

  async getAgentSkill(assistant: string, name: string): Promise<AgentSkillContent> {
    const response = await this.request(
      `/v1/agent-skills/${encodeURIComponent(assistant)}/${encodeURIComponent(name)}`,
      { method: "GET", authenticated: true, idempotent: true },
    );
    if (!isRecord(response)) {
      throw new Error("MemoryOps returned an unexpected agent skill response.");
    }
    return normalizeAgentSkillContent(response);
  }

  async createAgentSkill(input: AgentSkillCreateInput): Promise<AgentSkillContent> {
    const response = await this.request("/v1/agent-skills", {
      method: "POST",
      authenticated: true,
      body: input,
    });
    if (!isRecord(response)) {
      throw new Error("MemoryOps returned an unexpected agent skill response.");
    }
    return normalizeAgentSkillContent(response);
  }

  async updateAgentSkill(
    assistant: string,
    name: string,
    input: AgentSkillUpdateInput,
  ): Promise<AgentSkillContent> {
    const response = await this.request(
      `/v1/agent-skills/${encodeURIComponent(assistant)}/${encodeURIComponent(name)}`,
      { method: "PUT", authenticated: true, body: input },
    );
    if (!isRecord(response)) {
      throw new Error("MemoryOps returned an unexpected agent skill response.");
    }
    return normalizeAgentSkillContent(response);
  }

  async listAgentSkillVersions(assistant: string, name: string): Promise<AgentSkillVersion[]> {
    const response = await this.request(
      `/v1/agent-skills/${encodeURIComponent(assistant)}/${encodeURIComponent(name)}/versions`,
      { method: "GET", authenticated: true, idempotent: true },
    );
    return Array.isArray(response) ? response.filter(isRecord).map(normalizeAgentSkillVersion) : [];
  }

  async rollbackAgentSkillVersion(
    assistant: string,
    name: string,
    version: number,
    changeNote?: string,
  ): Promise<AgentSkillContent> {
    const response = await this.request(
      `/v1/agent-skills/${encodeURIComponent(assistant)}/${encodeURIComponent(name)}/versions/${version}/rollback`,
      {
        method: "POST",
        authenticated: true,
        body: changeNote ? { change_note: changeNote } : {},
      },
    );
    if (!isRecord(response)) {
      throw new Error("MemoryOps returned an unexpected rollback response.");
    }
    return normalizeAgentSkillContent(response);
  }

  async listContradictions(status?: string, after?: string): Promise<ContradictionListResponse> {
    const response = await this.request(
      `/v1/workspaces/${encodeURIComponent(this.config.workspaceId)}/contradictions${queryString({
        status,
        after,
        limit: 20,
      })}`,
      { method: "GET", authenticated: true, idempotent: true }
    );

    if (!isRecord(response)) {
      return { items: [], next_cursor: null };
    }

    const items = Array.isArray(response.items) ? response.items.filter(isRecord).map(normalizeContradictionItem) : [];
    return {
      items,
      next_cursor: typeof response.next_cursor === "string" ? response.next_cursor : null,
    };
  }

  async resolveContradiction(flagId: string, resolution: ContradictionResolution, notes?: string): Promise<ContradictionItem> {
    const response = await this.request(
      `/v1/workspaces/${encodeURIComponent(this.config.workspaceId)}/contradictions/${encodeURIComponent(flagId)}/resolve`,
      {
        method: "POST",
        authenticated: true,
        body: { resolution, notes: notes ?? null },
      }
    );
    return expectContradictionItem(response, "MemoryOps returned an unexpected contradiction resolution response.");
  }

  async bulkDismissContradictions(flagIds: string[], notes?: string): Promise<{ dismissed: number }> {
    const response = await this.request(
      `/v1/workspaces/${encodeURIComponent(this.config.workspaceId)}/contradictions/bulk-dismiss`,
      {
        method: "POST",
        authenticated: true,
        body: { flag_ids: flagIds, notes: notes ?? null },
      }
    );
    if (!isRecord(response)) {
      return { dismissed: 0 };
    }
    return {
      dismissed: numberOrDefault(response.dismissed, 0),
    };
  }

  async getContradictionCount(): Promise<{ open: number }> {
    const response = await this.request(
      `/v1/workspaces/${encodeURIComponent(this.config.workspaceId)}/contradictions/count`,
      { method: "GET", authenticated: true, idempotent: true }
    );
    if (!isRecord(response)) {
      return { open: 0 };
    }
    return {
      open: numberOrDefault(response.open, 0),
    };
  }

  private async request(path: string, options: {
    method: "GET" | "POST" | "PATCH" | "DELETE" | "PUT";
    authenticated: boolean;
    body?: unknown;
    // Mark a POST as safe to auto-retry (read-only endpoints like search/retrieve).
    idempotent?: boolean;
  }): Promise<unknown> {
    const maxRetries = Math.max(0, this.config.maxRetries ?? 0);
    const canRetry = RETRYABLE_METHODS.has(options.method) || options.idempotent === true;
    const attempts = canRetry ? maxRetries + 1 : 1;

    let lastError: unknown;
    for (let attempt = 1; attempt <= attempts; attempt++) {
      try {
        return await this.performRequest(path, options);
      } catch (error) {
        lastError = error;
        const transient = isTransientError(error);
        const hasMoreAttempts = attempt < attempts;
        if (!transient || !hasMoreAttempts) {
          throw error;
        }
        const delayMs = this.backoffDelayMs(attempt);
        this.log?.(`↻ ${options.method} ${path} → retry ${attempt}/${attempts - 1} in ${delayMs}ms (${error instanceof Error ? error.message : String(error)})`);
        await sleep(delayMs);
      }
    }

    throw lastError;
  }

  private backoffDelayMs(attempt: number): number {
    const base = Math.max(0, this.config.retryBackoffMs ?? 0);
    // Exponential backoff with light jitter, capped to keep the UI responsive.
    const exponential = base * Math.pow(2, attempt - 1);
    const jitter = exponential * 0.25 * Math.random();
    return Math.min(Math.round(exponential + jitter), 8000);
  }

  private async performRequest(path: string, options: {
    method: "GET" | "POST" | "PATCH" | "DELETE" | "PUT";
    authenticated: boolean;
    body?: unknown;
  }): Promise<unknown> {
    this.log?.(`→ ${options.method} ${path}`);
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

    const startTime = Date.now();
    try {
      const response = await fetch(`${this.config.apiUrl}${path}`, {
        method: options.method,
        headers,
        body: options.body === undefined ? undefined : JSON.stringify(options.body),
        signal: controller.signal,
      });

      const text = await response.text();
      const elapsed = Date.now() - startTime;
      const payload = text.length > 0 ? parseJson(text) : undefined;

      if (!response.ok) {
        const message = extractErrorMessage(payload) ?? response.statusText;
        this.log?.(`✗ ${options.method} ${path} → ${response.status} ${message} (${elapsed}ms)`);
        const error = new Error(`MemoryOps ${response.status}: ${message}`) as HttpError;
        error.status = response.status;
        throw error;
      }

      this.log?.(`← ${options.method} ${path} → ${response.status} (${elapsed}ms)`);
      return payload;
    } catch (error) {
      if (isAbortError(error)) {
        this.log?.(`✗ ${options.method} ${path} → timeout after ${Math.round(timeoutMs / 1000)}s`);
        const timeoutError = new Error(`MemoryOps request timed out after ${Math.round(timeoutMs / 1000)}s.`) as HttpError;
        timeoutError.transient = true;
        throw timeoutError;
      }
      // Avoid double-logging errors already logged above
      if (!(error instanceof Error && error.message.startsWith("MemoryOps "))) {
        this.log?.(`✗ ${options.method} ${path} → ${error instanceof Error ? error.message : String(error)}`);
        // Network-level failures (connection refused, DNS, reset) surface here.
        if (error instanceof Error) {
          (error as HttpError).transient = true;
        }
      }
      throw error;
    } finally {
      clearTimeout(timeout);
    }
  }

  private memoryPath(id: string, suffix = "", query: Record<string, unknown> = {}): string {
    return `/v1/memory/${encodeURIComponent(id)}${suffix}${queryString({
      workspace_id: this.config.workspaceId,
      ...query,
    })}`;
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

  const queryId = typeof response.query_id === "string" ? response.query_id : undefined;
  const results = response.results ?? response.items ?? response.memories;
  const normalized = Array.isArray(results) ? results.filter(isRecord).map(normalizeSearchItem) : [];

  if (queryId) {
    for (const item of normalized) {
      item.query_id = queryId;
    }
  }

  return normalized;
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
    score: numberOrUndefined(value.score),
    rank: numberOrUndefined(value.rank),
    importance_score: numberOrUndefined(value.importance_score),
    decay_score: numberOrUndefined(value.decay_score),
    relevance_score: numberOrUndefined(value.relevance_score),
    pinned: booleanOrUndefined(value.pinned),
    tags: stringArrayOrUndefined(value.tags),
    token_count: numberOrNullOrUndefined(value.token_count),
    source_events: stringArrayOrUndefined(value.source_events),
    source_episode_ids: stringArrayOrUndefined(value.source_episode_ids),
    corroboration_count: numberOrUndefined(value.corroboration_count),
    version: numberOrUndefined(value.version),
    promoted_at: stringOrNullOrUndefined(value.promoted_at),
    deleted_at: stringOrNullOrUndefined(value.deleted_at),
    created_at: stringOrUndefined(value.created_at),
    updated_at: stringOrUndefined(value.updated_at),
  };
}

function normalizeMemoryVersion(value: Record<string, unknown>): MemoryVersion {
  return {
    ...value,
    id: stringOrUndefined(value.id),
    memory_id: stringOrUndefined(value.memory_id),
    workspace_id: stringOrUndefined(value.workspace_id),
    version: numberOrUndefined(value.version),
    content: stringOrUndefined(value.content),
    importance_score: numberOrUndefined(value.importance_score),
    tags: stringArrayOrUndefined(value.tags),
    edited_by: stringOrUndefined(value.edited_by),
    created_at: stringOrUndefined(value.created_at),
  };
}

function normalizeProvenanceGraph(value: unknown): ProvenanceGraph {
  if (!isRecord(value)) {
    return { nodes: [], edges: [] };
  }

  return {
    ...value,
    root_id: stringOrUndefined(value.root_id),
    nodes: Array.isArray(value.nodes) ? value.nodes.filter(isRecord).map(normalizeProvenanceNode) : [],
    edges: Array.isArray(value.edges) ? value.edges.filter(isRecord).map(normalizeProvenanceEdge) : [],
  };
}

function normalizeProvenanceNode(value: Record<string, unknown>): ProvenanceNode {
  return {
    ...value,
    id: stringOrUndefined(value.id),
    node_type: stringOrUndefined(value.node_type),
    title: stringOrUndefined(value.title),
    subtitle: stringOrNullOrUndefined(value.subtitle),
    timestamp: stringOrNullOrUndefined(value.timestamp),
    metadata: isRecord(value.metadata) ? value.metadata : {},
  };
}

function normalizeProvenanceEdge(value: Record<string, unknown>): ProvenanceEdge {
  return {
    ...value,
    from: stringOrUndefined(value.from),
    to: stringOrUndefined(value.to),
    edge_type: stringOrUndefined(value.edge_type),
  };
}

function normalizeFeedbackResponse(value: unknown): FeedbackResponse {
  if (!isRecord(value)) {
    return { items: [], total: 0 };
  }

  const items = Array.isArray(value.items)
    ? value.items.filter(isRecord).map(normalizeFeedbackEntry)
    : [];

  return {
    ...value,
    items,
    total: numberOrDefault(value.total, items.length),
    memory_id: stringOrUndefined(value.memory_id),
    avg_rating: numberOrUndefined(value.avg_rating),
    relevance_score: numberOrUndefined(value.relevance_score),
  };
}

function normalizeFeedbackEntry(value: Record<string, unknown>): FeedbackEntry {
  return {
    ...value,
    id: stringOrUndefined(value.id),
    memory_id: stringOrUndefined(value.memory_id),
    query_id: stringOrUndefined(value.query_id),
    agent_id: stringOrNullOrUndefined(value.agent_id),
    user_id: stringOrNullOrUndefined(value.user_id),
    rating: numberOrDefault(value.rating, 0),
    comment: stringOrNullOrUndefined(value.comment),
    occurred_at: stringOrUndefined(value.occurred_at),
  };
}

function expectMemoryUnit(response: unknown, message: string): MemoryUnit {
  if (!isRecord(response)) {
    throw new Error(message);
  }

  return normalizeMemoryUnit(response);
}

function expectSkill(response: unknown, message: string): Skill {
  if (!isRecord(response)) {
    throw new Error(message);
  }
  return normalizeSkill(response);
}

function normalizeSkill(value: Record<string, unknown>): Skill {
  return {
    id: stringOrUndefined(value.id) ?? "",
    workspace_id: stringOrUndefined(value.workspace_id) ?? "",
    name: stringOrUndefined(value.name) ?? "",
    description: stringOrUndefined(value.description) ?? "",
    endpoint_url: stringOrUndefined(value.endpoint_url) ?? "",
    http_method: stringOrUndefined(value.http_method) ?? "POST",
    input_schema: value.input_schema ?? {},
    output_schema: value.output_schema ?? {},
    auth_header: stringOrNullOrUndefined(value.auth_header) ?? null,
    enabled: booleanOrUndefined(value.enabled) ?? false,
    version: numberOrDefault(value.version, 1),
    scope_visibility: skillScopeVisibilityOrDefault(value.scope_visibility),
    created_at: stringOrUndefined(value.created_at),
    updated_at: stringOrUndefined(value.updated_at),
  };
}

function normalizeSkillVersion(value: Record<string, unknown>): SkillVersion {
  return {
    id: stringOrUndefined(value.id) ?? "",
    skill_id: stringOrUndefined(value.skill_id) ?? "",
    workspace_id: stringOrUndefined(value.workspace_id) ?? "",
    name: stringOrUndefined(value.name) ?? "",
    version: numberOrDefault(value.version, 1),
    description: stringOrUndefined(value.description) ?? "",
    endpoint_url: stringOrUndefined(value.endpoint_url) ?? "",
    http_method: stringOrUndefined(value.http_method) ?? "POST",
    input_schema: value.input_schema ?? {},
    output_schema: value.output_schema ?? {},
    auth_header: stringOrNullOrUndefined(value.auth_header) ?? null,
    enabled: booleanOrUndefined(value.enabled) ?? false,
    scope_visibility: skillScopeVisibilityOrDefault(value.scope_visibility),
    change_note: stringOrNullOrUndefined(value.change_note) ?? null,
    created_by: stringOrNullOrUndefined(value.created_by) ?? null,
    created_at: stringOrUndefined(value.created_at),
  };
}

function skillScopeVisibilityOrDefault(value: unknown): "private" | "workspace" | "published" {
  if (value === "private" || value === "workspace" || value === "published") {
    return value;
  }
  return "workspace";
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
  return typeof value === "object" && value !== null && !Array.isArray(value);
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

function isTransientError(error: unknown): boolean {
  if (!isRecord(error)) {
    return false;
  }

  // Timeouts and network-level failures are flagged transient at the call site.
  if (error.transient === true) {
    return true;
  }

  // Retry only on transient HTTP status codes (5xx, 429, 408, 425).
  if (typeof error.status === "number") {
    return RETRYABLE_STATUS.has(error.status);
  }

  return false;
}

function sleep(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

function normalizeAgentSkill(value: Record<string, unknown>): AgentSkill {
  return {
    name: stringOrUndefined(value.name) ?? "",
    filename: stringOrUndefined(value.filename) ?? "",
    assistant: stringOrUndefined(value.assistant) ?? "",
    title: stringOrUndefined(value.title) ?? "",
    description: stringOrUndefined(value.description) ?? "",
    version: numberOrDefault(value.version, 1),
  };
}

function normalizeAgentSkillContent(value: Record<string, unknown>): AgentSkillContent {
  return {
    name: stringOrUndefined(value.name) ?? "",
    filename: stringOrUndefined(value.filename) ?? "",
    assistant: stringOrUndefined(value.assistant) ?? "",
    title: stringOrUndefined(value.title) ?? "",
    description: stringOrUndefined(value.description) ?? "",
    instructions: stringOrUndefined(value.instructions) ?? "",
    content: stringOrUndefined(value.content) ?? "",
    version: numberOrDefault(value.version, 1),
  };
}

function normalizeAgentSkillVersion(value: Record<string, unknown>): AgentSkillVersion {
  return {
    id: stringOrUndefined(value.id) ?? "",
    agent_skill_id: stringOrUndefined(value.agent_skill_id) ?? "",
    workspace_id: stringOrUndefined(value.workspace_id) ?? "",
    name: stringOrUndefined(value.name) ?? "",
    version: numberOrDefault(value.version, 1),
    assistant: stringOrUndefined(value.assistant) ?? "",
    title: stringOrUndefined(value.title) ?? "",
    description: stringOrUndefined(value.description) ?? "",
    instructions: stringOrUndefined(value.instructions) ?? "",
    content: stringOrUndefined(value.content) ?? "",
    change_note: stringOrNullOrUndefined(value.change_note) ?? null,
    created_by: stringOrNullOrUndefined(value.created_by) ?? null,
    created_at: stringOrUndefined(value.created_at) ?? "",
  };
}

function normalizeToolInvocation(value: Record<string, unknown>): ToolInvocation {
  return {
    id: numberOrDefault(value.id, 0),
    tool_id: stringOrUndefined(value.tool_id) ?? "",
    workspace_id: stringOrUndefined(value.workspace_id) ?? "",
    tool_name: stringOrUndefined(value.tool_name) ?? "",
    tool_version: numberOrDefault(value.tool_version, 1),
    actor: stringOrUndefined(value.actor) ?? "",
    source: stringOrUndefined(value.source) ?? "",
    status_code: numberOrDefault(value.status_code, 0),
    latency_ms: numberOrDefault(value.latency_ms, 0),
    error: stringOrNullOrUndefined(value.error) ?? null,
    occurred_at: stringOrUndefined(value.occurred_at) ?? "",
  };
}

function normalizeContradictionItem(value: Record<string, unknown>): ContradictionItem {
  return {
    id: stringOrUndefined(value.id) ?? "",
    workspace_id: stringOrUndefined(value.workspace_id) ?? "",
    memory_a: normalizeContradictionMemoryRef(value.memory_a),
    memory_b: normalizeContradictionMemoryRef(value.memory_b),
    similarity: numberOrDefault(value.similarity, 0),
    conflict_score: numberOrDefault(value.conflict_score, 0),
    resolution: stringOrUndefined(value.resolution) ?? "open",
    resolved_by: stringOrNullOrUndefined(value.resolved_by) ?? null,
    resolved_at: stringOrNullOrUndefined(value.resolved_at) ?? null,
    notes: stringOrNullOrUndefined(value.notes) ?? null,
    kept_memory_id: stringOrNullOrUndefined(value.kept_memory_id) ?? null,
    discarded_memory_id: stringOrNullOrUndefined(value.discarded_memory_id) ?? null,
    created_at: stringOrUndefined(value.created_at) ?? "",
  };
}

function normalizeContradictionMemoryRef(value: unknown): ContradictionMemoryRef {
  if (!isRecord(value)) {
    return { id: "", content_preview: "", created_at: "" };
  }
  return {
    id: stringOrUndefined(value.id) ?? "",
    content_preview: stringOrUndefined(value.content_preview) ?? "",
    created_at: stringOrUndefined(value.created_at) ?? "",
  };
}

function expectContradictionItem(value: unknown, errorMsg: string): ContradictionItem {
  if (!isRecord(value)) {
    throw new Error(errorMsg);
  }
  return normalizeContradictionItem(value);
}
