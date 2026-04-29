import { apiRequest, queryString } from "./client";

export interface ContradictionMemoryRef {
  id: string;
  content_preview: string;
  created_at: string;
}

export interface ContradictionItem {
  id: string;
  memory_a: ContradictionMemoryRef;
  memory_b: ContradictionMemoryRef;
  similarity: number;
  conflict_score: number;
  resolution: string;
  notes: string | null;
  created_at: string;
}

export async function listContradictions(
  workspaceId: string,
  status?: string,
  after?: string,
): Promise<{ items: ContradictionItem[]; next_cursor: string | null }> {
  return apiRequest<{ items: ContradictionItem[]; next_cursor: string | null }>(
    `/v1/workspaces/${workspaceId}/contradictions${queryString({ status, after, limit: 20 })}`,
  );
}

export async function resolveContradiction(
  workspaceId: string,
  flagId: string,
  resolution: "accepted" | "dismissed",
  notes?: string,
): Promise<ContradictionItem> {
  return apiRequest<ContradictionItem>(`/v1/workspaces/${workspaceId}/contradictions/${flagId}/resolve`, {
    method: "POST",
    body: { resolution, notes: notes ?? null },
  });
}

export async function getContradictionCount(workspaceId: string): Promise<{ open: number }> {
  return apiRequest<{ open: number }>(`/v1/workspaces/${workspaceId}/contradictions/count`);
}
