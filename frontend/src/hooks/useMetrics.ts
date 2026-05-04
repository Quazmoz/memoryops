import { useQuery } from "@tanstack/react-query";

import type { MetricsSnapshot } from "../api/metrics";
import { hasWorkspaceAuth } from "../lib/auth";
import { useAppStore } from "../store/app-store";

export function useMetrics(workspaceId: string) {
  const apiKey = useAppStore((state) => state.apiKey);

  return useQuery<MetricsSnapshot | null>({
    queryKey: ["workspace", workspaceId, "metrics"],
    queryFn: async () => null,
    enabled: hasWorkspaceAuth(workspaceId, apiKey),
    refetchInterval: false,
    staleTime: Number.POSITIVE_INFINITY,
    retry: false,
  });
}
