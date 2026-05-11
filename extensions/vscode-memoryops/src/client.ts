import { MemoryOpsConfig } from "./config";

export interface MemorySearchResult {
  id?: string;
  content?: string;
  memory_type?: string;
  source?: string;
  tags?: string[];
  score?: number;
  [key: string]: unknown;
}

export interface RetrievalMemory {
  id?: string;
  content?: string;
  score?: number;
  token_count?: number;
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

  async searchMemory(query: string, topK: number): Promise<MemorySearchResult[]> {
    const response = await this.request("/v1/memory/search", {
      method: "POST",
      authenticated: true,
      body: {
        query,
        workspace_id: this.config.workspaceId,
        top_k: topK,
      },
    });

    if (Array.isArray(response)) {
      return response as MemorySearchResult[];
    }

    if (isRecord(response)) {
      const results = response.results ?? response.items ?? response.memories;
      if (Array.isArray(results)) {
        return results as MemorySearchResult[];
      }
    }

    return [];
  }

  async retrieve(query: string, tokenBudget: number): Promise<RetrievalResult> {
    const response = await this.request("/v1/retrieve", {
      method: "POST",
      authenticated: true,
      body: {
        query,
        workspace_id: this.config.workspaceId,
        token_budget: tokenBudget,
      },
    });

    return isRecord(response) ? (response as RetrievalResult) : {};
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
    method: "GET" | "POST";
    authenticated: boolean;
    body?: unknown;
  }): Promise<unknown> {
    const headers: Record<string, string> = {
      Accept: "application/json",
    };

    if (options.body !== undefined) {
      headers["Content-Type"] = "application/json";
    }

    if (options.authenticated) {
      headers["X-API-Key"] = this.config.apiKey;
    }

    const response = await fetch(`${this.config.apiUrl}${path}`, {
      method: options.method,
      headers,
      body: options.body === undefined ? undefined : JSON.stringify(options.body),
    });

    const text = await response.text();
    const payload = text.length > 0 ? parseJson(text) : undefined;

    if (!response.ok) {
      const message = extractErrorMessage(payload) ?? response.statusText;
      throw new Error(`MemoryOps ${response.status}: ${message}`);
    }

    return payload;
  }
}

function parseJson(text: string): unknown {
  try {
    return JSON.parse(text);
  } catch {
    return text;
  }
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
