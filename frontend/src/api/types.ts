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
  embedding_id?: string | null;
  token_count?: number | null;
  source_events: string[];
  source_episode_ids: string[];
  corroboration_count: number;
  promoted_at?: string | null;
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
  memory_types?: MemoryType[];
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

export type RetrieveRequest = {
  query: string;
  workspace_id: string;
  scope?: MemoryScope;
  token_budget?: number;
  mode?: SearchMode;
  include_trace?: boolean;
};

export type ScoreBreakdown = {
  semantic_similarity: number;
  keyword_rank: number;
  importance: number;
  recency: number;
  source_authority: number;
};

export type PackedMemory = {
  id: string;
  content: string;
  memory_type: MemoryType | string;
  importance_score: number;
  decay_score: number;
  entities: MemoryEntity[];
  score_breakdown: ScoreBreakdown;
};

export type RetrievalTraceEntry = {
  memory_id: string;
  score: number;
  included: boolean;
  exclusion_reason?: string | null;
  score_breakdown: ScoreBreakdown;
};

export type RetrievalTrace = {
  query_id: string;
  query: string;
  mode: SearchMode;
  candidates_evaluated: number;
  included_count: number;
  excluded_count: number;
  entries: RetrievalTraceEntry[];
};

export type RetrieveResponse = {
  query_id: string;
  memories: PackedMemory[];
  total_tokens: number;
  trace?: RetrievalTrace | null;
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

export type CreateWorkspaceResponse = {
  id?: string;
  name?: string;
  workspace_id?: string;
};

export type WorkspaceSummary = {
  id: string;
  name: string;
};

export type WorkspaceConfig = {
  promotion_threshold?: number;
  dedup_cosine_threshold?: number;
  [key: string]: JsonValue | undefined;
};

export type WorkspaceDetail = WorkspaceSummary & {
  config: WorkspaceConfig | JsonValue;
  promotion_threshold: number;
  dedup_cosine_threshold: number;
  created_at?: string;
  updated_at?: string;
  deleted_at?: string | null;
};

export type WorkspaceStats = {
  total_memories: number;
  episodic_count: number;
  semantic_count: number;
  pinned_count: number;
  deleted_count: number;
  avg_importance_score: number;
  avg_decay_score: number;
  memories_created_7d: number;
  memories_created_30d: number;
  oldest_memory_at: string | null;
  newest_memory_at: string | null;
};

export type PromotionReport = {
  clusters_found: number;
  units_promoted: number;
  units_skipped: number;
};

export type CreateApiKeyResponse = {
  id?: string;
  name?: string;
  prefix?: string;
  key?: string;
  plaintext_key?: string;
};

export type CreatedApiKey = {
  plaintext_key: string;
};

export type AuditEvent = {
  id: string;
  workspace_id: string;
  actor: string;
  action: string;
  target_id: string;
  target_type: string;
  diff?: JsonValue | null;
  occurred_at: string;
};

export type AuditEntry = AuditEvent;

export type AuditResponse = {
  items: AuditEvent[];
  limit: number;
  offset: number;
};

export type IntegrationStatus = "active" | "degraded" | "failing" | string;

export type IntegrationResponse = {
  source: string;
  last_event_at?: string | null;
  events_24h: number;
  errors_24h: number;
  status: IntegrationStatus;
};

export type DlqEntryResponse = {
  job_id: string;
  workspace_id?: string;
  payload_summary: string;
  error?: string;
  error_message?: string;
  retry_count?: number;
  attempts?: number;
  failed_at?: string | null;
  created_at?: string | null;
};

export type DlqEntry = {
  job_id: string;
  workspace_id: string;
  error_message: string;
  attempts: number;
  created_at?: string | null;
};
