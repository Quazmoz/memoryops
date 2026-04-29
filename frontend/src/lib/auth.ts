export function hasWorkspaceAuth(workspaceId: string, apiKey: string): boolean {
  return workspaceId.trim().length > 0 && apiKey.trim().length > 0;
}
