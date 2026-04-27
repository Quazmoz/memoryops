export type JsonPrimitive = string | number | boolean | null;
export type JsonValue = JsonPrimitive | JsonValue[] | { [key: string]: JsonValue };

export type MemoryType = "episodic" | "semantic";
export type MemoryTypeFilter = "all" | MemoryType;
export type SearchMode = "hybrid" | "vector" | "keyword";
export type SortField = "importance_score" | "decay_score" | "updated_at" | "created_at";
export type SortDirection = "asc" | "desc";

export type EntityType = "person" | "repo" | "branch" | "file" | "team" | "topic";

export type MemoryEntity = {
  entity_type: EntityType | string;
  value: string;
  confidence?: number;
};

export type MemoryScope = {
  workspace_id?: string;
  agent_id?: string | null;
  user_id?: string | null;
  repo?: string | null;
};

export type MemoryUnit = {
  id: string;
  workspace_id: string;
  scope: MemoryScope | JsonValue | null;
  memory_type: MemoryType;
  content: string;
  entities?: MemoryEntity[];
  importance_score: number;
  importance_overridden?: boolean;
  decay_score: number;
  pinned: boolean;
  tags: string[];
  token_count?: number | null;
  source_events: string[];
  access_count?: number;
  created_at: string;
  updated_at: string;
};

export type ListMemoryResponse = {
  items: MemoryUnit[];
  total: number;
  limit: number;
  offset: number;
};

export type SearchFilters = {
  memory_type?: MemoryType;
  source?: string;
  min_importance?: number;
  pinned?: boolean;
  tags?: string[];
};

export type SearchRequest = {
  query: string;
  workspace_id: string;
  mode: SearchMode;
  limit?: number;
  offset?: number;
  filters?: SearchFilters;
};

export type SearchResult = {
  memory: MemoryUnit;
  score: number;
  rank: number;
};

export type SearchResponse = {
  results: SearchResult[];
  total: number;
  query_id: string;
};

export type UpdateMemoryRequest = {
  pinned?: boolean;
  tags?: string[];
  importance_score?: number;
};

export type ReadinessResponse = {
  status: "ok" | "unavailable" | string;
  checks?: {
    database?: string;
    redis?: string;
    qdrant?: string;
  };
  httpStatus: number;
};

export type IngestAcceptedResponse = {
  status: string;
  event_id?: string | null;
};

export type IngestResult = {
  ok: boolean;
  status: number;
  data: IngestAcceptedResponse | JsonValue | null;
  detail?: string;
};

export type ProviderDefaults = {
  embedding: {
    provider: string;
    model: string;
  };
  llm: {
    provider: string;
    model: string;
    baseUrl: string;
  };
};
