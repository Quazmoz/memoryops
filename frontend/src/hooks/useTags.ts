import { useQuery } from "@tanstack/react-query";

import { listWorkspaceTags } from "../api/workspaces";
import { useAppStore } from "../store/app-store";

export function useTags(workspaceId: string) {
  const apiKey = useAppStore((state) => state.apiKey);

  return useQuery({
    queryKey: ["tags", workspaceId],
    queryFn: () => listWorkspaceTags(workspaceId),
    enabled: workspaceId.trim().length > 0 && apiKey.trim().length > 0,
    staleTime: 60_000,
  });
}
