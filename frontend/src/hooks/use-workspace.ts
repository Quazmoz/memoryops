import { useMutation, useQuery } from "@tanstack/react-query";

import { getRetrievalTrace, postRetrieve } from "../api/memory";
import type { RetrieveRequest } from "../api/types";
import { getWorkspaceStats } from "../api/workspaces";
import { useAppStore } from "../store/app-store";

export function useWorkspaceStats(workspaceId: string) {
  const apiKey = useAppStore((state) => state.apiKey);

  return useQuery({
    queryKey: ["workspace", workspaceId, "stats"],
    queryFn: () => getWorkspaceStats(workspaceId),
    enabled: workspaceId.trim().length > 0 && apiKey.trim().length > 0,
    staleTime: 30_000,
    refetchInterval: 60_000,
  });
}

export function useRetrieve() {
  const workspaceId = useAppStore((state) => state.workspaceId);

  return useMutation({
    mutationKey: ["workspace", workspaceId, "retrieve"],
    mutationFn: (request: RetrieveRequest) => postRetrieve(workspaceId, request),
  });
}

export function useRetrievalTrace(queryId: string) {
  const workspaceId = useAppStore((state) => state.workspaceId);

  return useQuery({
    queryKey: ["trace", workspaceId, queryId],
    queryFn: () => getRetrievalTrace(workspaceId, queryId),
    enabled: queryId.trim().length > 0,
    staleTime: Infinity,
  });
}
