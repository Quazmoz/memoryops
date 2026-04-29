import { useQuery } from "@tanstack/react-query";

import { listWorkspaceTags } from "../api/workspaces";
import { hasWorkspaceAuth } from "../lib/auth";
import { useAppStore } from "../store/app-store";

export function useTags(workspaceId: string) {
  const apiKey = useAppStore((state) => state.apiKey);

  return useQuery({
    queryKey: ["tags", workspaceId],
    queryFn: () => listWorkspaceTags(workspaceId),
    enabled: hasWorkspaceAuth(workspaceId, apiKey),
    staleTime: 60_000,
  });
}
