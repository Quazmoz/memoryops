import { useQuery } from "@tanstack/react-query";

import { getSystemHealth } from "../api/health";
import { listIntegrations } from "../api/integrations";

export const LIVE_QUERY_INTERVALS = {
  audit: 30_000,
  systemHealth: 30_000,
  readiness: 30_000,
  integrations: 60_000,
  workspaceStats: 120_000,
} as const;

export function liveRefetchInterval(enabled: boolean, intervalMs: number): number | false {
  return enabled ? intervalMs : false;
}

export function useSystemHealth(enabled: boolean) {
  return useQuery({
    queryKey: ["health", "system"],
    queryFn: getSystemHealth,
    enabled,
    staleTime: LIVE_QUERY_INTERVALS.systemHealth,
    refetchInterval: liveRefetchInterval(enabled, LIVE_QUERY_INTERVALS.systemHealth),
    refetchIntervalInBackground: false,
  });
}

export function useWorkspaceIntegrations(workspaceId: string, enabled: boolean) {
  const canFetch = enabled && workspaceId.trim().length > 0;

  return useQuery({
    queryKey: ["workspace", workspaceId, "integrations"],
    queryFn: () => listIntegrations(workspaceId),
    enabled: canFetch,
    staleTime: LIVE_QUERY_INTERVALS.integrations,
    refetchInterval: liveRefetchInterval(canFetch, LIVE_QUERY_INTERVALS.integrations),
    refetchIntervalInBackground: false,
  });
}