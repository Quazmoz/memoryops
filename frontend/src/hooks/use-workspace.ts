import { useMutation, useQuery } from "@tanstack/react-query";

import { getRetrievalTrace, postRetrieve } from "../api/memory";
import type { RetrieveRequest } from "../api/types";
import { getWorkspaceStats, getWorkspaceStatsHistory } from "../api/workspaces";
import { hasWorkspaceAuth } from "../lib/auth";
import { useAppStore } from "../store/app-store";

export function useWorkspaceStats(workspaceId: string) {
  const apiKey = useAppStore((state) => state.apiKey);

  return useQuery({
    queryKey: ["workspace", workspaceId, "stats"],
    queryFn: () => getWorkspaceStats(workspaceId),
    enabled: hasWorkspaceAuth(workspaceId, apiKey),
    staleTime: 30_000,
    refetchInterval: 120_000,
    retry: false,
  });
}

export function useStatsHistory(workspaceId: string, days = 30) {
  const apiKey = useAppStore((state) => state.apiKey);

  return useQuery({
    queryKey: ["workspace", workspaceId, "stats", "history", days],
    queryFn: () => getWorkspaceStatsHistory(workspaceId, days),
    enabled: hasWorkspaceAuth(workspaceId, apiKey),
    staleTime: 5 * 60 * 1000,
    retry: false,
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
    enabled: queryId.trim() !== "",
    staleTime: Infinity,
  });
}
