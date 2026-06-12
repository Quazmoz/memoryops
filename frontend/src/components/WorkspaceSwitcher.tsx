import { useQuery } from "@tanstack/react-query";

import { listWorkspaces } from "../api/workspaces";
import { useAppStore } from "../store/app-store";

/**
 * Sidebar control showing the active workspace and, when the API key can see
 * more than one workspace, a selector to switch between them while keeping
 * the current API key.
 */
export function WorkspaceSwitcher() {
  const workspaceId = useAppStore((state) => state.workspaceId);
  const apiKey = useAppStore((state) => state.apiKey);
  const setWorkspaceId = useAppStore((state) => state.setWorkspaceId);

  const workspacesQuery = useQuery({
    queryKey: ["workspaces", "list"],
    queryFn: () => listWorkspaces(apiKey),
    enabled: apiKey.trim().length > 0,
    staleTime: 60_000,
    retry: false,
  });

  const workspaces = workspacesQuery.data ?? [];
  const active = workspaces.find((workspace) => workspace.id === workspaceId);

  if (apiKey.trim().length === 0) {
    return null;
  }

  return (
    <div className="border-b border-line px-5 py-3" data-testid="workspace-switcher">
      <p className="text-[10px] font-semibold uppercase tracking-wide text-ink/45">Workspace</p>
      {workspaces.length > 1 ? (
        <select
          data-testid="workspace-switcher-select"
          aria-label="Switch workspace"
          value={workspaceId}
          onChange={(event) => setWorkspaceId(event.target.value)}
          className="mt-1 w-full rounded-md border border-line bg-white px-2 py-1.5 text-sm text-ink outline-none focus:border-accent focus:ring-2 focus:ring-accent/20"
        >
          {workspaces.map((workspace) => (
            <option key={workspace.id} value={workspace.id}>
              {workspace.name}
            </option>
          ))}
          {active ? null : (
            <option value={workspaceId}>{truncatedId(workspaceId)}</option>
          )}
        </select>
      ) : (
        <p className="mt-1 truncate text-sm font-medium text-ink" title={active?.name ?? workspaceId}>
          {active?.name ?? workspaces[0]?.name ?? truncatedId(workspaceId)}
        </p>
      )}
    </div>
  );
}

function truncatedId(value: string): string {
  return value.length > 12 ? `${value.slice(0, 8)}…` : value;
}
