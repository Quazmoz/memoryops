import {
  type QueryClient,
  type QueryKey,
  useMutation,
  useQuery,
  useQueryClient,
} from "@tanstack/react-query";

import {
  buildSearchRequest,
  getMemory,
  getReadiness,
  listMemory,
  patchMemory,
  searchMemory,
  type MemoryListParams,
  type SearchCriteria,
} from "../api/memory";
import type { ListMemoryResponse, MemoryUnit, SearchResponse, UpdateMemoryRequest } from "../api/types";
import { validateImportanceScore } from "../lib/validation";
import { useAppStore } from "../store/app-store";

export const memoryKeys = {
  workspace: (workspaceId: string) => ["workspace", workspaceId] as const,
  readiness: (workspaceId: string) => ["workspace", workspaceId, "readiness"] as const,
  all: (workspaceId: string) => ["workspace", workspaceId, "memory"] as const,
  lists: (workspaceId: string) => ["workspace", workspaceId, "memory", "list"] as const,
  list: (workspaceId: string, params: MemoryListParams) => ["workspace", workspaceId, "memory", "list", params] as const,
  detail: (workspaceId: string, id: string) => ["workspace", workspaceId, "memory", "detail", id] as const,
  searches: (workspaceId: string) => ["workspace", workspaceId, "memory", "search"] as const,
  search: (workspaceId: string, criteria: SearchCriteria) => ["workspace", workspaceId, "memory", "search", criteria] as const,
};

type OptimisticContext = {
  snapshots: Array<[QueryKey, unknown]>;
};

export function useReadiness(workspaceId: string) {
  return useQuery({
    queryKey: memoryKeys.readiness(workspaceId),
    queryFn: getReadiness,
    refetchInterval: 10_000,
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

export function useMemorySearch(workspaceId: string, criteria: SearchCriteria) {
  const apiKey = useAppStore((state) => state.apiKey);

  return useQuery({
    queryKey: memoryKeys.search(workspaceId, criteria),
    queryFn: () => searchMemory(buildSearchRequest(workspaceId, criteria)),
    enabled: hasWorkspaceAuth(workspaceId, apiKey) && criteria.query.trim().length > 0,
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

export function optimisticallyPatchMemoryCaches(
  queryClient: QueryClient,
  workspaceId: string,
  memoryId: string,
  patch: UpdateMemoryRequest | MemoryUnit,
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

export function applyMemoryPatch(memory: MemoryUnit, patch: UpdateMemoryRequest | MemoryUnit): MemoryUnit {
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

function isMemoryUnit(value: UpdateMemoryRequest | MemoryUnit): value is MemoryUnit {
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

function hasWorkspaceAuth(workspaceId: string, apiKey: string): boolean {
  return workspaceId.trim().length > 0 && apiKey.trim().length > 0;
}
