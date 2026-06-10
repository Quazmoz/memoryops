import { AlertTriangle, Loader2, Trash2 } from "lucide-react";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { useNavigate } from "react-router-dom";

import { deleteWorkspace } from "../api/workspaces";
import { useAppStore } from "../store/app-store";
import { InlineError } from "./InlineError";
import { Button } from "./ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "./ui/card";
import { Input } from "./ui/input";

type WorkspaceDangerZoneProps = {
  workspaceId: string;
  workspaceName?: string | undefined;
  disabled?: boolean;
};

/**
 * Settings "Danger Zone" card. Deleting a workspace requires typing the
 * workspace name (or the workspace ID when the name is unavailable) so the
 * destructive action cannot be triggered accidentally.
 */
export function WorkspaceDangerZone({ workspaceId, workspaceName, disabled = false }: WorkspaceDangerZoneProps) {
  const clearWorkspace = useAppStore((state) => state.clearWorkspace);
  const queryClient = useQueryClient();
  const navigate = useNavigate();
  const [confirmOpen, setConfirmOpen] = useState(false);
  const [confirmText, setConfirmText] = useState("");

  const expectedConfirmation = (workspaceName ?? "").trim().length > 0 ? (workspaceName ?? "").trim() : workspaceId;
  const confirmationMatches = confirmText.trim() === expectedConfirmation;

  const deleteMutation = useMutation({
    mutationKey: ["workspace", workspaceId, "delete-workspace"],
    mutationFn: () => deleteWorkspace(workspaceId),
    onSuccess: () => {
      // The workspace and its API keys are gone; drop credentials and cached
      // data, then send the operator back to the first-run setup flow.
      clearWorkspace();
      queryClient.clear();
      navigate("/", { replace: true });
    },
  });

  function requestDelete() {
    setConfirmText("");
    setConfirmOpen(true);
  }

  function cancelDelete() {
    setConfirmText("");
    setConfirmOpen(false);
  }

  function confirmDelete() {
    if (!confirmationMatches || deleteMutation.isPending) {
      return;
    }
    deleteMutation.mutate();
  }

  return (
    <Card className="border-rust/40" data-testid="danger-zone-card">
      <CardHeader className="flex flex-row items-center justify-between space-y-0">
        <CardTitle className="flex items-center gap-1.5 text-rust">Danger Zone</CardTitle>
        <AlertTriangle className="h-4 w-4 text-rust" aria-hidden="true" />
      </CardHeader>
      <CardContent className="grid gap-4">
        <div className="flex flex-col gap-3 sm:flex-row sm:items-start sm:justify-between">
          <div>
            <p className="text-sm font-medium text-ink">Delete this workspace</p>
            <p className="mt-1 text-xs text-ink/60">
              Soft-deletes the workspace, revokes all API keys, and removes memories from the vector index. This
              disconnects every agent and integration using this workspace.
            </p>
          </div>
          <Button
            type="button"
            variant="destructive"
            data-testid="delete-workspace-button"
            disabled={disabled || workspaceId.trim().length === 0 || deleteMutation.isPending || confirmOpen}
            onClick={requestDelete}
          >
            <Trash2 className="h-4 w-4" aria-hidden="true" />
            Delete Workspace
          </Button>
        </div>

        {deleteMutation.isError ? (
          <InlineError title="Workspace deletion failed" message={deleteMutation.error.message} />
        ) : null}

        {confirmOpen ? (
          <div className="grid gap-3 rounded-lg border border-rust/30 bg-orange-50 p-4" data-testid="delete-workspace-confirm">
            <p className="text-sm font-medium text-ink">
              This cannot be undone from the Control Center. Type{" "}
              <code className="break-all rounded bg-white px-1 py-0.5 font-mono text-xs">{expectedConfirmation}</code>{" "}
              to confirm.
            </p>
            <Input
              data-testid="delete-workspace-confirm-input"
              value={confirmText}
              onChange={(event) => setConfirmText(event.target.value)}
              placeholder={expectedConfirmation}
              autoComplete="off"
              disabled={deleteMutation.isPending}
            />
            <div className="flex justify-end gap-2">
              <Button
                type="button"
                variant="ghost"
                size="sm"
                data-testid="delete-workspace-cancel"
                disabled={deleteMutation.isPending}
                onClick={cancelDelete}
              >
                Cancel
              </Button>
              <Button
                type="button"
                variant="destructive"
                size="sm"
                data-testid="delete-workspace-confirm-button"
                disabled={!confirmationMatches || deleteMutation.isPending}
                onClick={confirmDelete}
              >
                {deleteMutation.isPending ? (
                  <Loader2 className="h-3.5 w-3.5 animate-spin" aria-hidden="true" />
                ) : (
                  <Trash2 className="h-3.5 w-3.5" aria-hidden="true" />
                )}
                Permanently delete workspace
              </Button>
            </div>
          </div>
        ) : null}
      </CardContent>
    </Card>
  );
}
