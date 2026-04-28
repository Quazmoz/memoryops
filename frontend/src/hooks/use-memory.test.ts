import { QueryClient } from "@tanstack/react-query";

import type { ListMemoryResponse, MemoryUnit, SearchResponse } from "../api/types";
import { memoryKeys, optimisticallyPatchMemoryCaches } from "./use-memory";

describe("optimisticallyPatchMemoryCaches", () => {
  it("updates detail, list, and search cache entries for a PATCH", () => {
    const queryClient = new QueryClient();
    const workspaceId = "018f0000-0000-7000-8000-000000000001";
    const memory = memoryFactory({ id: "mem-1", pinned: false, importance_score: 0.4, importance_overridden: false });
    const listParams = { limit: 50, offset: 0, memoryType: "all" as const };
    const searchCriteria = {
      query: "retrieval",
      memoryType: "all" as const,
      pinned: false,
      minImportance: 0,
      tags: [] as string[],
      limit: 50,
      offset: 0,
    };

    queryClient.setQueryData(memoryKeys.detail(workspaceId, memory.id), memory);
    queryClient.setQueryData<ListMemoryResponse>(memoryKeys.list(workspaceId, listParams), {
      items: [memory],
      total: 1,
      limit: 50,
      offset: 0,
    });
    queryClient.setQueryData<SearchResponse>(memoryKeys.search(workspaceId, searchCriteria), {
      results: [{ memory, score: 0.82, rank: 1 }],
      total: 1,
      query_id: "query-1",
    });

    optimisticallyPatchMemoryCaches(queryClient, workspaceId, memory.id, {
      pinned: true,
      importance_score: 0.91,
    });

    const detail = queryClient.getQueryData<MemoryUnit>(memoryKeys.detail(workspaceId, memory.id));
    const list = queryClient.getQueryData<ListMemoryResponse>(memoryKeys.list(workspaceId, listParams));
    const search = queryClient.getQueryData<SearchResponse>(memoryKeys.search(workspaceId, searchCriteria));

    expect(detail?.pinned).toBe(true);
    expect(detail?.importance_score).toBe(0.91);
    expect(detail?.importance_overridden).toBe(true);
    expect(list?.items[0]?.pinned).toBe(true);
    expect(search?.results[0]?.memory.importance_score).toBe(0.91);
  });
});

function memoryFactory(overrides: Partial<MemoryUnit> = {}): MemoryUnit {
  return {
    id: "mem-1",
    workspace_id: "018f0000-0000-7000-8000-000000000001",
    scope: {
      workspace_id: "018f0000-0000-7000-8000-000000000001",
      agent_id: null,
      user_id: null,
      repo: "Quazmoz/memoryops",
    },
    memory_type: "episodic",
    content: "Mona opened a retrieval scoring pull request.",
    entities: [{ entity_type: "person", value: "mona", confidence: 0.99 }],
    importance_score: 0.4,
    importance_overridden: false,
    decay_score: 0.88,
    pinned: false,
    tags: ["retrieval"],
    token_count: 12,
    source_events: ["event-1"],
    source_episode_ids: [],
    corroboration_count: 1,
    promoted_at: null,
    access_count: 2,
    created_at: "2026-04-27T15:20:30Z",
    updated_at: "2026-04-27T15:20:30Z",
    ...overrides,
  };
}
