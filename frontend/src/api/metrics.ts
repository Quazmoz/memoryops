import { apiRequest } from "./client";

export interface MetricsValues {
  ingest_events_total: number | null;
  slow_path_jobs_processed: number | null;
  slow_path_jobs_failed: number | null;
  retrieval_requests_total: number | null;
  embedding_latency_p50_ms: number | null;
  embedding_latency_p99_ms: number | null;
  llm_latency_p50_ms: number | null;
  llm_latency_p99_ms: number | null;
  token_pack_budget_used_pct: number | null;
}

export interface MetricsSnapshot {
  workspace_id: string;
  collected_at: string;
  metrics?: MetricsValues;
  ingest_events_total?: number | null;
  slow_path_jobs_processed?: number | null;
  slow_path_jobs_failed?: number | null;
  retrieval_requests_total?: number | null;
  embedding_latency_p50_ms?: number | null;
  embedding_latency_p99_ms?: number | null;
  llm_latency_p50_ms?: number | null;
  llm_latency_p99_ms?: number | null;
  token_pack_budget_used_pct?: number | null;
}

export function fetchMetrics(workspaceId: string): Promise<MetricsSnapshot> {
  return apiRequest<MetricsSnapshot>(`/v1/workspaces/${workspaceId}/metrics`);
}
