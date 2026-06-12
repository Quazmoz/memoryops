import {
  type QueryClient,
  type QueryKey,
  useMutation,
  useQuery,
  useQueryClient,
} from "@tanstack/react-query";

import {
  buildSearchRequest,
  deleteMemory,
  getMemoryFeedback,
  getMemory,
  getMemoryHistory,
  getMemoryProvenance,
  getReadiness,
  listMemory,
  mergeMemories,
  patchMemory,
  promoteMemory,
  publishMemory,
  restoreMemory,
  searchMemory,
  submitFeedback,
  bulkMemory,
  type MemoryListParams,
  type SearchCriteria,
} from "../api/memory";
import type { FeedbackResponse, ListMemoryResponse, MemoryUnit, MemoryVersion, MergeMemoryRequest, SearchResponse, SubmitFeedbackRequest, UpdateMemoryRequest, BulkMemoryRequest, BulkMemoryResponse } from "../api/types";
import { hasWorkspaceAuth } from "../lib/auth";
import { validateImportanceScore } from "../lib/validation";
import { useAppStore } from "../store/app-store";
import { LIVE_QUERY_INTERVALS, liveRefetchInterval } from "./use-live-query";

export const memoryKeys = {
  workspace: (workspaceId: string) => ["workspace", workspaceId] as const,
  readiness: (workspaceId: string) => ["workspace", workspaceId, "readiness"] as const,
  all: (workspaceId: string) => ["workspace", workspaceId, "memory"] as const,
  lists: (workspaceId: string) => ["workspace", workspaceId, "memory", "list"] as const,
  list: (workspaceId: string, params: MemoryListParams) => ["workspace", workspaceId, "memory", "list", params] as const,
  detail: (workspaceId: string, id: string) => ["workspace", workspaceId, "memory", "detail", id] as const,
  provenance: (workspaceId: string, id: string) => ["workspace", workspaceId, "memory", "provenance", id] as const,
  searches: (workspaceId: string) => ["workspace", workspaceId, "memory", "search"] as const,
  search: (workspaceId: string, criteria: SearchCriteria) => ["workspace", workspaceId, "memory", "search", criteria] as const,
};

type OptimisticContext = {
  snapshots: Array<[QueryKey, unknown]>;
};

type MemoryCachePatch = UpdateMemoryRequest | Partial<MemoryUnit> | MemoryUnit;

export function useReadiness(workspaceId: string) {
  const apiKey = useAppStore((state) => state.apiKey);
  const enabled = hasWorkspaceAuth(workspaceId, apiKey);

  return useQuery({
    queryKey: memoryKeys.readiness(workspaceId),
    queryFn: getReadiness,
    refetchInterval: liveRefetchInterval(enabled, LIVE_QUERY_INTERVALS.readiness),
    refetchIntervalInBackground: false,
    enabled,
  });
}

export function useMemoryList(workspaceId: string, params: MemoryListParams) {
  const apiKey = useAppStore((state) => state.apiKey);

  return useQuery({
    queryKey: memoryKeys.list(workspaceId, params),
    queryFn: () => listMemory(workspaceId, params),
    enabled: hasWorkspaceAuth(workspaceId, apiKey),
  });
}

export function useMemoryDetail(workspaceId: string, id: string | undefined) {
  const apiKey = useAppStore((state) => state.apiKey);

  return useQuery({
    queryKey: memoryKeys.detail(workspaceId, id ?? "missing"),
    queryFn: () => getMemory(workspaceId, id ?? ""),
    enabled: hasWorkspaceAuth(workspaceId, apiKey) && Boolean(id?.trim()),
  });
}

export function useMemoryProvenance(workspaceId: string, id: string | undefined) {
  const apiKey = useAppStore((state) => state.apiKey);

  return useQuery({
    queryKey: memoryKeys.provenance(workspaceId, id ?? "missing"),
    queryFn: () => getMemoryProvenance(workspaceId, id ?? ""),
    enabled: hasWorkspaceAuth(workspaceId, apiKey) && Boolean(id?.trim()),
  });
}

export function useMemorySearch(workspaceId: string, criteria: SearchCriteria) {
  const apiKey = useAppStore((state) => state.apiKey);

  return useQuery({
    queryKey: memoryKeys.search(workspaceId, criteria),
    queryFn: () => searchMemory(buildSearchRequest(workspaceId, criteria)),
    enabled: hasWorkspaceAuth(workspaceId, apiKey) && criteria.query.trim() !== "",
  });
}

export function useUpdateMemory(workspaceId: string) {
  const queryClient = useQueryClient();

  return useMutation<MemoryUnit, Error, { id: string; patch: UpdateMemoryRequest }, OptimisticContext>({
    mutationKey: ["workspace", workspaceId, "memory", "update"],
    mutationFn: ({ id, patch }) => patchMemory(workspaceId, id, patch),
    onMutate: async ({ id, patch }) => {
      validatePatch(patch);
      await queryClient.cancelQueries({ queryKey: memoryKeys.all(workspaceId) });
      const snapshots = queryClient.getQueriesData({ queryKey: memoryKeys.all(workspaceId) });
      optimisticallyPatchMemoryCaches(queryClient, workspaceId, id, patch);
      return { snapshots };
    },
    onError: (_error, _variables, context) => {
      context?.snapshots.forEach(([queryKey, data]) => {
        queryClient.setQueryData(queryKey, data);
      });
    },
    onSuccess: (memory) => {
      queryClient.setQueryData(memoryKeys.detail(workspaceId, memory.id), memory);
      optimisticallyPatchMemoryCaches(queryClient, workspaceId, memory.id, memory);
    },
    onSettled: (_data, _error, variables) => {
      void queryClient.invalidateQueries({ queryKey: memoryKeys.detail(workspaceId, variables.id) });
      void queryClient.invalidateQueries({ queryKey: memoryKeys.lists(workspaceId) });
      void queryClient.invalidateQueries({ queryKey: memoryKeys.searches(workspaceId) });
    },
  });
}

export function usePublishMemory(workspaceId: string) {
  const queryClient = useQueryClient();

  return useMutation<MemoryUnit, Error, { id: string }, OptimisticContext>({
    mutationKey: ["workspace", workspaceId, "memory", "publish"],
    mutationFn: ({ id }) => publishMemory(workspaceId, id),
    onMutate: async ({ id }) => {
      await queryClient.cancelQueries({ queryKey: memoryKeys.all(workspaceId) });
      const snapshots = queryClient.getQueriesData({ queryKey: memoryKeys.all(workspaceId) });
      optimisticallyPatchMemoryCaches(queryClient, workspaceId, id, { scope_visibility: "workspace" });
      return { snapshots };
    },
    onError: (_error, _variables, context) => {
      context?.snapshots.forEach(([queryKey, data]) => {
        queryClient.setQueryData(queryKey, data);
      });
    },
    onSuccess: (memory) => {
      queryClient.setQueryData(memoryKeys.detail(workspaceId, memory.id), memory);
      optimisticallyPatchMemoryCaches(queryClient, workspaceId, memory.id, memory);
    },
    onSettled: (_data, _error, variables) => {
      void queryClient.invalidateQueries({ queryKey: memoryKeys.detail(workspaceId, variables.id) });
      void queryClient.invalidateQueries({ queryKey: memoryKeys.lists(workspaceId) });
      void queryClient.invalidateQueries({ queryKey: memoryKeys.searches(workspaceId) });
    },
  });
}

export function useSubmitFeedback(workspaceId: string) {
  const queryClient = useQueryClient();

  return useMutation<MemoryUnit, Error, { memoryId: string; request: SubmitFeedbackRequest }>({
    mutationKey: ["workspace", workspaceId, "memory", "feedback"],
    mutationFn: ({ memoryId, request }) => submitFeedback(workspaceId, memoryId, request),
    onSuccess: (memory) => {
      queryClient.setQueryData(memoryKeys.detail(workspaceId, memory.id), memory);
      optimisticallyPatchMemoryCaches(queryClient, workspaceId, memory.id, memory);
    },
    onSettled: (_data, _error, variables) => {
      void queryClient.invalidateQueries({
        queryKey: [...memoryKeys.detail(workspaceId, variables.memoryId), "feedback"],
      });
    },
  });
}

export function useMemoryFeedback(
  workspaceId: string,
  memoryId: string | undefined,
  params: { limit?: number; offset?: number } = {},
) {
  const apiKey = useAppStore((state) => state.apiKey);

  return useQuery<FeedbackResponse>({
    queryKey: [...memoryKeys.detail(workspaceId, memoryId ?? "missing"), "feedback", params],
    queryFn: () => getMemoryFeedback(workspaceId, memoryId ?? "", params),
    enabled: hasWorkspaceAuth(workspaceId, apiKey) && Boolean(memoryId?.trim()),
    staleTime: 30_000,
  });
}

export function optimisticallyPatchMemoryCaches(
  queryClient: QueryClient,
  workspaceId: string,
  memoryId: string,
  patch: MemoryCachePatch,
): void {
  queryClient.setQueryData<MemoryUnit | undefined>(memoryKeys.detail(workspaceId, memoryId), (current) =>
    current ? applyMemoryPatch(current, patch) : current,
  );

  queryClient.setQueriesData<ListMemoryResponse | undefined>({ queryKey: memoryKeys.lists(workspaceId) }, (current) => {
    if (!current) {
      return current;
    }

    return {
      ...current,
      items: current.items.map((memory) => (memory.id === memoryId ? applyMemoryPatch(memory, patch) : memory)),
    };
  });

  queryClient.setQueriesData<SearchResponse | undefined>({ queryKey: memoryKeys.searches(workspaceId) }, (current) => {
    if (!current) {
      return current;
    }

    return {
      ...current,
      results: current.results.map((result) => ({
        ...result,
        memory: result.memory.id === memoryId ? applyMemoryPatch(result.memory, patch) : result.memory,
      })),
    };
  });
}

export function applyMemoryPatch(memory: MemoryUnit, patch: MemoryCachePatch): MemoryUnit {
  if (isMemoryUnit(patch)) {
    return {
      ...memory,
      ...patch,
    };
  }

  const next: MemoryUnit = {
    ...memory,
    ...patch,
  };

  if ("importance_score" in patch && typeof patch.importance_score === "number") {
    next.importance_overridden = true;
  }

  return next;
}

function isMemoryUnit(value: MemoryCachePatch): value is MemoryUnit {
  return "content" in value && "workspace_id" in value;
}

function validatePatch(patch: UpdateMemoryRequest): void {
  if (patch.importance_score === undefined) {
    return;
  }

  const message = validateImportanceScore(patch.importance_score);
  if (message) {
    throw new Error(message);
  }
}

export function useMemoryHistory(workspaceId: string, id: string | undefined) {
  const apiKey = useAppStore((state) => state.apiKey);

  return useQuery<MemoryVersion[]>({
    queryKey: [...memoryKeys.detail(workspaceId, id ?? "missing"), "history"],
    queryFn: () => getMemoryHistory(workspaceId, id ?? ""),
    enabled: hasWorkspaceAuth(workspaceId, apiKey) && Boolean(id?.trim()),
    staleTime: 30_000,
  });
}

function invalidateLifecycleQueries(queryClient: QueryClient, workspaceId: string, memoryId: string): void {
  void queryClient.invalidateQueries({ queryKey: memoryKeys.detail(workspaceId, memoryId) });
  void queryClient.invalidateQueries({ queryKey: memoryKeys.lists(workspaceId) });
  void queryClient.invalidateQueries({ queryKey: memoryKeys.searches(workspaceId) });
  void queryClient.invalidateQueries({ queryKey: ["workspace", workspaceId, "stats"] });
  void queryClient.invalidateQueries({ queryKey: ["workspace", workspaceId, "dashboard"] });
  void queryClient.invalidateQueries({ queryKey: ["workspace", workspaceId, "lifecycle"] });
}

export function useDeleteMemory(workspaceId: string) {
  const queryClient = useQueryClient();

  return useMutation<MemoryUnit, Error, { id: string }>({
    mutationKey: ["workspace", workspaceId, "memory", "delete"],
    mutationFn: ({ id }) => deleteMemory(workspaceId, id),
    onSuccess: (_memory, variables) => {
      queryClient.removeQueries({ queryKey: memoryKeys.detail(workspaceId, variables.id) });
    },
    onSettled: (_data, _error, variables) => {
      invalidateLifecycleQueries(queryClient, workspaceId, variables.id);
    },
  });
}

export function useRestoreMemory(workspaceId: string) {
  const queryClient = useQueryClient();

  return useMutation<MemoryUnit, Error, { id: string }>({
    mutationKey: ["workspace", workspaceId, "memory", "restore"],
    mutationFn: ({ id }) => restoreMemory(workspaceId, id),
    onSuccess: (memory) => {
      queryClient.setQueryData(memoryKeys.detail(workspaceId, memory.id), memory);
    },
    onSettled: (_data, _error, variables) => {
      invalidateLifecycleQueries(queryClient, workspaceId, variables.id);
    },
  });
}

export function usePromoteMemory(workspaceId: string) {
  const queryClient = useQueryClient();

  return useMutation<MemoryUnit, Error, { id: string }>({
    mutationKey: ["workspace", workspaceId, "memory", "promote"],
    mutationFn: ({ id }) => promoteMemory(workspaceId, id),
    onSuccess: (memory) => {
      queryClient.setQueryData(memoryKeys.detail(workspaceId, memory.id), memory);
      optimisticallyPatchMemoryCaches(queryClient, workspaceId, memory.id, memory);
    },
    onSettled: (_data, _error, variables) => {
      invalidateLifecycleQueries(queryClient, workspaceId, variables.id);
    },
  });
}

export function useMergeMemories(workspaceId: string) {
  const queryClient = useQueryClient();

  return useMutation<MemoryUnit, Error, MergeMemoryRequest>({
    mutationKey: ["workspace", workspaceId, "memory", "merge"],
    mutationFn: (request) => mergeMemories(workspaceId, request),
    onSuccess: (memory) => {
      queryClient.setQueryData(memoryKeys.detail(workspaceId, memory.id), memory);
    },
    onSettled: (_data, _error, variables) => {
      queryClient.removeQueries({ queryKey: memoryKeys.detail(workspaceId, variables.source_id) });
      invalidateLifecycleQueries(queryClient, workspaceId, variables.target_id);
    },
  });
}

export function useBulkMemory(workspaceId: string) {
  const queryClient = useQueryClient();

  return useMutation<BulkMemoryResponse, Error, BulkMemoryRequest, OptimisticContext>({
    mutationKey: ["workspace", workspaceId, "memory", "bulk"],
    mutationFn: (request) => bulkMemory(workspaceId, request),
    onMutate: async (request) => {
      await queryClient.cancelQueries({ queryKey: memoryKeys.all(workspaceId) });
      const snapshots = queryClient.getQueriesData({ queryKey: memoryKeys.all(workspaceId) });
      
      const idsSet = new Set(request.ids);

      if (request.action === "pin" || request.action === "unpin") {
        const pinnedValue = request.action === "pin";
        request.ids.forEach(id => {
          optimisticallyPatchMemoryCaches(queryClient, workspaceId, id, { pinned: pinnedValue });
        });
      } else if (request.action === "delete") {
        request.ids.forEach(id => {
          queryClient.setQueryData(memoryKeys.detail(workspaceId, id), undefined);
        });

        queryClient.setQueriesData<ListMemoryResponse | undefined>({ queryKey: memoryKeys.lists(workspaceId) }, (current) => {
          if (!current) return current;
          return {
            ...current,
            items: current.items.filter(item => !idsSet.has(item.id)),
            total: Math.max(0, current.total - current.items.filter(item => idsSet.has(item.id)).length),
          };
        });

        queryClient.setQueriesData<SearchResponse | undefined>({ queryKey: memoryKeys.searches(workspaceId) }, (current) => {
          if (!current) return current;
          return {
            ...current,
            results: current.results.filter(result => !idsSet.has(result.memory.id)),
            total: Math.max(0, current.total - current.results.filter(result => idsSet.has(result.memory.id)).length),
          };
        });
      }

      return { snapshots };
    },
    onError: (_error, _variables, context) => {
      context?.snapshots.forEach(([queryKey, data]) => {
        queryClient.setQueryData(queryKey, data);
      });
    },
    onSuccess: (response) => {
      if (response.action === "pin" || response.action === "unpin") {
        const pinnedValue = response.action === "pin";
        response.affected_ids.forEach(id => {
          const detailKey = memoryKeys.detail(workspaceId, id);
          const current = queryClient.getQueryData<MemoryUnit>(detailKey);
          if (current) {
            queryClient.setQueryData(detailKey, { ...current, pinned: pinnedValue });
          }
        });
      } else if (response.action === "delete") {
        response.affected_ids.forEach(id => {
          queryClient.setQueryData(memoryKeys.detail(workspaceId, id), undefined);
        });
      }
    },
    onSettled: (_data, _error, variables) => {
      variables.ids.forEach(id => {
        void queryClient.invalidateQueries({ queryKey: memoryKeys.detail(workspaceId, id) });
      });
      void queryClient.invalidateQueries({ queryKey: memoryKeys.lists(workspaceId) });
      void queryClient.invalidateQueries({ queryKey: memoryKeys.searches(workspaceId) });
      void queryClient.invalidateQueries({ queryKey: ["workspace", workspaceId, "tags"] });
      void queryClient.invalidateQueries({ queryKey: ["workspace", workspaceId, "stats"] });
      void queryClient.invalidateQueries({ queryKey: ["workspace", workspaceId, "dashboard"] });
    },
  });
}

