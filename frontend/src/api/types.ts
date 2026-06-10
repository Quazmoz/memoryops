export type JsonPrimitive = string | number | boolean | null;
export type JsonValue = JsonPrimitive | JsonValue[] | { [key: string]: JsonValue };

export type MemoryType = "episodic" | "semantic";
export type ScopeVisibility = "private" | "workspace";
export type MemoryTypeFilter = "all" | MemoryType;
export type SearchMode = "hybrid" | "vector" | "keyword";
export type SortField = "importance_score" | "decay_score" | "relevance_score" | "updated_at" | "created_at";
export type SortDirection = "asc" | "desc";
export type FeedbackRating = -1 | 0 | 1;

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

export type ScopeFilter = {
  agent_id?: string;
  user_id?: string;
  repo?: string;
};

export type MemoryUnit = {
  id: string;
  workspace_id: string;
  scope: MemoryScope | JsonValue | null;
  memory_type: MemoryType;
  scope_visibility: ScopeVisibility;
  content: string;
  entities?: MemoryEntity[];
  importance_score: number;
  importance_overridden?: boolean;
  decay_score: number;
  relevance_score: number;
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
  agent_id?: string;
  user_id?: string;
  repo?: string;
};

export type SearchRequest = {
  query: string;
  workspace_id: string;
  mode: SearchMode;
  limit?: number;
  offset?: number;
  filters?: SearchFilters;
  scope?: ScopeFilter;
  agent_id?: string;
  user_id?: string;
  repo?: string;
  memory_types?: MemoryType[];
  as_of?: string;
  include_workspace_pool?: boolean;
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
  limit?: number;
  scope?: ScopeFilter;
  agent_id?: string;
  user_id?: string;
  repo?: string;
  token_budget?: number;
  search_mode?: SearchMode;
  mode?: SearchMode;
  include_trace?: boolean;
  as_of?: string;
  include_workspace_pool?: boolean;
};

export type TagSummary = {
  name: string;
  count: number;
};

export type TagsResponse = {
  tags: TagSummary[];
  next_cursor?: string | null;
};

export type FeedbackEntry = {
  id: string;
  memory_id: string;
  query_id: string;
  agent_id?: string | null;
  user_id?: string | null;
  rating: FeedbackRating;
  comment?: string | null;
  occurred_at: string;
};

export type FeedbackResponse = {
  items: FeedbackEntry[];
  total: number;
  memory_id: string;
  avg_rating: number;
  relevance_score: number;
};

export type SubmitFeedbackRequest = {
  query_id: string;
  rating: FeedbackRating;
  agent_id?: string;
  user_id?: string;
  comment?: string;
};

export type ImportMemoriesResponse = {
  imported: number;
  skipped: number;
  errors: number;
};

export type ScoreBreakdown = {
  semantic_similarity: number;
  keyword_rank: number;
  importance: number;
  recency: number;
  source_authority: number;
};

export type PackedMemory = {
  id?: string;
  memory_id?: string;
  content: string;
  memory_type: MemoryType | string;
  importance_score: number;
  decay_score: number;
  relevance_score?: number;
  rrf_score?: number;
  token_count?: number | null;
  tags?: string[];
  created_at?: string;
  entities?: MemoryEntity[];
  score_breakdown?: ScoreBreakdown;
};

export type TraceCandidate = {
  memory_id: string;
  content_snippet?: string;
  memory_type?: MemoryType | string;
  keyword_score?: number | null;
  vector_score?: number | null;
  rrf_score?: number;
  decay_score?: number;
  relevance_score?: number;
  importance_score?: number;
  final_score?: number;
  token_count?: number | null;
  score?: number;
  included: boolean;
  exclusion_reason?: string | null;
  score_breakdown?: ScoreBreakdown;
};

export type RetrievalTraceEntry = TraceCandidate;

export type RetrievalTrace = {
  query_id: string;
  query?: string;
  query_text?: string;
  as_of?: string | null;
  mode?: SearchMode;
  feedback_applied?: boolean;
  search_mode?: string;
  created_at?: string;
  elapsed_ms?: number;
  total_candidates?: number;
  candidates_evaluated?: number;
  included_count: number;
  excluded_count?: number;
  token_budget?: number;
  token_count?: number;
  entries?: RetrievalTraceEntry[];
  candidates?: TraceCandidate[];
};

export type ProvenanceNode = {
  id: string;
  node_type: "raw_event" | "memory" | "merge" | "access" | string;
  title: string;
  subtitle: string | null;
  timestamp: string | null;
  metadata: JsonValue;
};

export type ProvenanceEdge = {
  from: string;
  to: string;
  edge_type: "created_from" | "promoted_to" | "merged_into" | "accessed_as" | string;
};

export type ProvenanceGraph = {
  root_id: string;
  nodes: ProvenanceNode[];
  edges: ProvenanceEdge[];
};

export type RetrieveResponse = {
  query_id: string;
  memories?: PackedMemory[];
  items?: PackedMemory[];
  total_tokens?: number;
  total_candidates?: number;
  token_count?: number;
  elapsed_ms?: number;
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

export type CreateWorkspaceResponse = {
  id?: string;
  name?: string;
  workspace_id?: string;
  api_key?: string;
};

export type WorkspaceSummary = {
  id: string;
  name: string;
  api_key?: string;
};

export type WorkspaceConfig = {
  promotion_threshold?: number;
  dedup_cosine_threshold?: number;
  decay_half_life_days?: number;
  pruning_threshold?: number;
  retention_max_age_days?: number | null;
  skill_version_retention_days?: number | null;
  compliance_hard_purge?: boolean;
  compliance_mode?: boolean;
  contradiction_mode?: "quarantine" | "auto_resolve" | string;
  contradiction_threshold?: number;
  contradiction_candidates?: number;
  sub_agent_pools?: string[];
  llm_provider?: string;
  llm_model?: string;
  llm_base_url?: string | null;
  llm_api_key_env?: string | null;
  embedding_provider?: string;
  embedding_model?: string;
  [key: string]: JsonValue | undefined;
};

export type WorkspaceDetail = WorkspaceSummary &
  WorkspaceConfig & {
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

export interface StatsHistoryPoint {
  date: string;
  created: number;
  promoted: number;
  soft_deleted: number;
}

export interface StatsHistory {
  days: number;
  series: StatsHistoryPoint[];
}

export type PromotionReport = {
  clusters_found: number;
  units_promoted: number;
  units_skipped: number;
};

export type ForgetUserDataResponse = {
  user_id: string;
  memories_purged: number;
  raw_events_purged: number;
  mode: "hard_purge" | "soft_delete" | string;
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

export type AuditResponse = {
  items: AuditEvent[];
  limit: number;
  offset: number;
  next_cursor?: string | null;
};

export type IntegrationStatus = "active" | "degraded" | "failing" | string;

export type IntegrationResponse = {
  source: string;
  last_event_at?: string | null;
  events_24h: number;
  errors_24h: number;
  status: IntegrationStatus;
};

export type MemoryVersion = {
  id: string;
  memory_id: string;
  workspace_id: string;
  version: number;
  content: string;
  importance_score: number;
  tags: string[];
  edited_by: string;
  created_at: string;
};

export type MergeMemoryRequest = {
  source_id: string;
  target_id: string;
};

export type BulkMemoryAction = "pin" | "unpin" | "delete";
export type BulkMemoryRequest = { ids: string[]; action: BulkMemoryAction };
export type BulkMemoryResponse = {
  affected: number;
  affected_ids: string[];
  requested: number;
  action: BulkMemoryAction;
};

export type ApiKeySummary = {
  id: string;
  name: string;
  prefix: string;
  created_at: string;
  last_used_at: string | null;
  revoked: boolean;
};

