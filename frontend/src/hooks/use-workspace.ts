import { useQuery } from "@tanstack/react-query";

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
