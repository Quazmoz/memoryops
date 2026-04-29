import { useQuery } from "@tanstack/react-query";

import { fetchMetrics } from "../api/metrics";
import { hasWorkspaceAuth } from "../lib/auth";
import { useAppStore } from "../store/app-store";

export function useMetrics(workspaceId: string) {
  const apiKey = useAppStore((state) => state.apiKey);

  return useQuery({
    queryKey: ["workspace", workspaceId, "metrics"],
    queryFn: () => fetchMetrics(workspaceId),
    enabled: hasWorkspaceAuth(workspaceId, apiKey),
    refetchInterval: 30_000,
    staleTime: 25_000,
  });
}
