import { ArrowUpCircle, Loader2, RotateCcw, Trash2 } from "lucide-react";
import { useState } from "react";

import type { MemoryUnit } from "../api/types";
import { useDeleteMemory, usePromoteMemory, useRestoreMemory } from "../hooks/use-memory";
import { InlineError } from "./InlineError";
import { Button } from "./ui/button";
import { Tooltip, TooltipContent, TooltipTrigger } from "./ui/tooltip";

type MemoryLifecycleActionsProps = {
  workspaceId: string;
  memory: MemoryUnit;
};

/**
 * Promote / delete / restore controls for a single memory. Delete is a
 * backend soft-delete, so a successful delete switches the panel into a
 * "deleted" state offering restore (available for 30 days server-side).
 */
export function MemoryLifecycleActions({ workspaceId, memory }: MemoryLifecycleActionsProps) {
  const promoteMemory = usePromoteMemory(workspaceId);
  const deleteMemory = useDeleteMemory(workspaceId);
  const restoreMemory = useRestoreMemory(workspaceId);
  const [confirmDeleteOpen, setConfirmDeleteOpen] = useState(false);
  const [deleted, setDeleted] = useState(false);

  function confirmDelete() {
    deleteMemory.mutate(
      { id: memory.id },
      {
        onSuccess: () => {
          setConfirmDeleteOpen(false);
          setDeleted(true);
        },
      },
    );
  }

  function restore() {
    restoreMemory.mutate(
      { id: memory.id },
      {
        onSuccess: () => {
          setDeleted(false);
        },
      },
    );
  }

  if (deleted) {
    return (
      <div className="grid gap-2" data-testid="memory-deleted-banner">
        <div className="flex flex-wrap items-center gap-3 rounded-md border border-amber-200 bg-amber-50 px-3 py-2 text-sm text-amber-900" role="status">
          <span className="font-medium">Memory deleted.</span>
          <span>It can be restored within 30 days.</span>
          <Button
            type="button"
            variant="secondary"
            size="sm"
            data-testid="memory-restore-button"
            disabled={restoreMemory.isPending}
            onClick={restore}
          >
            {restoreMemory.isPending ? <Loader2 className="h-4 w-4 animate-spin" aria-hidden="true" /> : <RotateCcw className="h-4 w-4" aria-hidden="true" />}
            Restore
          </Button>
        </div>
        {restoreMemory.isError ? <InlineError title="Restore failed" message={restoreMemory.error.message} /> : null}
      </div>
    );
  }

  return (
    <div className="grid gap-2">
      <div className="flex flex-wrap justify-end gap-2">
        {memory.memory_type === "episodic" ? (
          <Tooltip>
            <TooltipTrigger asChild>
              <Button
                type="button"
                variant="secondary"
                data-testid="memory-promote-button"
                disabled={promoteMemory.isPending}
                onClick={() => promoteMemory.mutate({ id: memory.id })}
              >
                {promoteMemory.isPending ? <Loader2 className="h-4 w-4 animate-spin" aria-hidden="true" /> : <ArrowUpCircle className="h-4 w-4" aria-hidden="true" />}
                Promote
              </Button>
            </TooltipTrigger>
            <TooltipContent>Force-promotes this episodic memory into durable semantic memory without waiting for a lifecycle pass.</TooltipContent>
          </Tooltip>
        ) : null}
        <Tooltip>
          <TooltipTrigger asChild>
            <Button
              type="button"
              variant="destructive"
              data-testid="memory-delete-button"
              disabled={deleteMemory.isPending || confirmDeleteOpen}
              onClick={() => setConfirmDeleteOpen(true)}
            >
              <Trash2 className="h-4 w-4" aria-hidden="true" />
              Delete
            </Button>
          </TooltipTrigger>
          <TooltipContent>Soft-deletes this memory. It stays restorable for 30 days, then is purged.</TooltipContent>
        </Tooltip>
      </div>

      {promoteMemory.isError ? <InlineError title="Promote failed" message={promoteMemory.error.message} /> : null}
      {deleteMemory.isError ? <InlineError title="Delete failed" message={deleteMemory.error.message} /> : null}

      {confirmDeleteOpen ? (
        <div className="grid gap-2 rounded-lg border border-rust/30 bg-orange-50 p-3" data-testid="memory-delete-confirm">
          <p className="text-sm text-ink">Delete this memory? It will disappear from retrieval immediately and can be restored for 30 days.</p>
          <div className="flex justify-end gap-2">
            <Button
              type="button"
              variant="ghost"
              size="sm"
              data-testid="memory-delete-cancel"
              disabled={deleteMemory.isPending}
              onClick={() => setConfirmDeleteOpen(false)}
            >
              Cancel
            </Button>
            <Button
              type="button"
              variant="destructive"
              size="sm"
              data-testid="memory-delete-confirm-button"
              disabled={deleteMemory.isPending}
              onClick={confirmDelete}
            >
              {deleteMemory.isPending ? <Loader2 className="h-3.5 w-3.5 animate-spin" aria-hidden="true" /> : <Trash2 className="h-3.5 w-3.5" aria-hidden="true" />}
              Delete memory
            </Button>
          </div>
        </div>
      ) : null}
    </div>
  );
}
