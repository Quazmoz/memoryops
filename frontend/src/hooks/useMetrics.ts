import { useQuery } from "@tanstack/react-query";

import { fetchMetrics } from "../api/metrics";
import { useAppStore } from "../store/app-store";

export function useMetrics(workspaceId: string) {
  const apiKey = useAppStore((state) => state.apiKey);

  return useQuery({
    queryKey: ["workspace", workspaceId, "metrics"],
    queryFn: () => fetchMetrics(workspaceId),
    enabled: workspaceId.trim().length > 0 && apiKey.trim().length > 0,
    refetchInterval: 30_000,
    staleTime: 25_000,
  });
}
