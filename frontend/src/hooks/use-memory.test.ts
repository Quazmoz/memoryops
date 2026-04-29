import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, renderHook, waitFor } from "@testing-library/react";
import { createElement, type ReactNode } from "react";
import { vi } from "vitest";

import { getMemoryFeedback, submitFeedback } from "../api/memory";
import type { ListMemoryResponse, MemoryUnit, SearchResponse, SubmitFeedbackRequest } from "../api/types";
import { useAppStore } from "../store/app-store";
import { memoryKeys, optimisticallyPatchMemoryCaches, useMemoryFeedback, useSubmitFeedback } from "./use-memory";

vi.mock("../api/memory", async (importOriginal) => {
  const actual = await importOriginal<typeof import("../api/memory")>();

  return {
    ...actual,
    getMemoryFeedback: vi.fn(),
    submitFeedback: vi.fn(),
  };
});

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

describe("useSubmitFeedback", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    useAppStore.setState({ apiKey: "test-api-key", workspaceId: "018f0000-0000-7000-8000-000000000001" });
  });

  it("calls submitFeedback with correct args and updates cache", async () => {
    const queryClient = createQueryClient();
    const workspaceId = "018f0000-0000-7000-8000-000000000001";
    const memory = memoryFactory({ id: "mem-feedback", relevance_score: 0.5 });
    const updated = memoryFactory({ id: "mem-feedback", relevance_score: 1 });
    const listParams = { limit: 50, offset: 0, memoryType: "all" as const };
    const request: SubmitFeedbackRequest = { query_id: "query-1", rating: 1, comment: "useful" };

    vi.mocked(submitFeedback).mockResolvedValue(updated);
    queryClient.setQueryData(memoryKeys.detail(workspaceId, memory.id), memory);
    queryClient.setQueryData<ListMemoryResponse>(memoryKeys.list(workspaceId, listParams), {
      items: [memory],
      total: 1,
      limit: 50,
      offset: 0,
    });

    const { result } = renderHook(() => useSubmitFeedback(workspaceId), { wrapper: queryWrapper(queryClient) });

    act(() => {
      result.current.mutate({ memoryId: memory.id, request });
    });

    await waitFor(() => expect(submitFeedback).toHaveBeenCalledWith(workspaceId, memory.id, request));
    await waitFor(() => {
      const detail = queryClient.getQueryData<MemoryUnit>(memoryKeys.detail(workspaceId, memory.id));
      expect(detail?.relevance_score).toBe(1);
    });

    const list = queryClient.getQueryData<ListMemoryResponse>(memoryKeys.list(workspaceId, listParams));
    expect(list?.items[0]?.relevance_score).toBe(1);
  });
});

describe("useMemoryFeedback", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    useAppStore.setState({ apiKey: "test-api-key", workspaceId: "018f0000-0000-7000-8000-000000000001" });
  });

  it("is disabled when memoryId is empty", () => {
    const queryClient = createQueryClient();
    const workspaceId = "018f0000-0000-7000-8000-000000000001";

    const { result } = renderHook(() => useMemoryFeedback(workspaceId, "", { limit: 5, offset: 0 }), {
      wrapper: queryWrapper(queryClient),
    });

    expect(result.current.fetchStatus).toBe("idle");
    expect(getMemoryFeedback).not.toHaveBeenCalled();
  });
});

function createQueryClient(): QueryClient {
  return new QueryClient({
    defaultOptions: {
      queries: { retry: false },
      mutations: { retry: false },
    },
  });
}

function queryWrapper(queryClient: QueryClient) {
  return function Wrapper({ children }: { children: ReactNode }) {
    return createElement(QueryClientProvider, { client: queryClient }, children);
  };
}

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
    scope_visibility: "private",
    content: "Mona opened a retrieval scoring pull request.",
    entities: [{ entity_type: "person", value: "mona", confidence: 0.99 }],
    importance_score: 0.4,
    importance_overridden: false,
    decay_score: 0.88,
    relevance_score: 0.5,
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
