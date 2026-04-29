import { CheckCircle2, ChevronDown, Loader2, ShieldBan, Trash2, XCircle } from "lucide-react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useEffect, useState } from "react";

import {
  bulkDismissContradictions,
  listContradictions,
  resolveContradiction,
  type ContradictionItem,
  type ContradictionResolution,
} from "../api/contradictions";
import { EmptyState } from "../components/EmptyState";
import { InlineError } from "../components/InlineError";
import { Badge } from "../components/ui/badge";
import { Button } from "../components/ui/button";
import { Skeleton } from "../components/ui/skeleton";
import { formatDateTime, formatRelativeTime, formatScore } from "../lib/format";
import { cn } from "../lib/utils";
import { useAppStore } from "../store/app-store";

const tabs = [
  { value: "open", label: "Open" },
  { value: "auto_resolved", label: "Auto-resolved" },
  { value: "keep_a", label: "Keep A" },
  { value: "keep_b", label: "Keep B" },
  { value: "dismissed", label: "Dismissed" },
  { value: "accepted", label: "Accepted" },
];

export function ContradictionsView() {
  const workspaceId = useAppStore((state) => state.workspaceId);
  const apiKey = useAppStore((state) => state.apiKey);
  const queryClient = useQueryClient();
  const [status, setStatus] = useState("open");
  const [items, setItems] = useState<ContradictionItem[]>([]);
  const [after, setAfter] = useState<string | undefined>();
  const [nextCursor, setNextCursor] = useState<string | null>(null);
  const [resolveTarget, setResolveTarget] = useState<string | null>(null);
  const [notes, setNotes] = useState("");
  const [hoverResolution, setHoverResolution] = useState<ContradictionResolution | null>(null);
  const [selectedIds, setSelectedIds] = useState<Set<string>>(new Set());
  const hasAuth = workspaceId.trim().length > 0 && apiKey.trim().length > 0;

  const contradictionsQuery = useQuery({
    queryKey: ["workspace", workspaceId, "contradictions", status, after],
    queryFn: () => listContradictions(workspaceId, status, after),
    enabled: hasAuth,
  });

  const resolveMutation = useMutation({
    mutationKey: ["workspace", workspaceId, "contradictions", "resolve"],
    mutationFn: ({ id, resolution, notes: n }: { id: string; resolution: ContradictionResolution; notes?: string }) =>
      resolveContradiction(workspaceId, id, resolution, n),
    onSuccess: (resolved) => {
      setItems((current) => current.filter((item) => item.id !== resolved.id));
      setResolveTarget(null);
      setNotes("");
      setHoverResolution(null);
      void queryClient.invalidateQueries({ queryKey: ["workspace", workspaceId, "contradictions"] });
      void queryClient.invalidateQueries({ queryKey: ["workspace", workspaceId, "contradictions", "count"] });
    },
  });

  const bulkDismissMutation = useMutation({
    mutationKey: ["workspace", workspaceId, "contradictions", "bulk-dismiss"],
    mutationFn: (flagIds: string[]) => bulkDismissContradictions(workspaceId, flagIds),
    onSuccess: ({ dismissed }) => {
      setItems((current) => current.filter((item) => !selectedIds.has(item.id)));
      setSelectedIds(new Set());
      void queryClient.invalidateQueries({ queryKey: ["workspace", workspaceId, "contradictions"] });
      void queryClient.invalidateQueries({ queryKey: ["workspace", workspaceId, "contradictions", "count"] });
      traceLog(`Dismissed ${dismissed} flag(s)`);
    },
  });

  useEffect(() => {
    if (!contradictionsQuery.data) {
      return;
    }
    setItems((current) => (after ? [...current, ...contradictionsQuery.data.items] : contradictionsQuery.data.items));
    setNextCursor(contradictionsQuery.data.next_cursor);
  }, [after, contradictionsQuery.data]);

  function selectStatus(nextStatus: string) {
    setStatus(nextStatus);
    setItems([]);
    setAfter(undefined);
    setNextCursor(null);
    setResolveTarget(null);
    setNotes("");
    setSelectedIds(new Set());
  }

  function submitResolution(id: string, resolution: ContradictionResolution) {
    const trimmedNotes = notes.trim();
    resolveMutation.mutate(trimmedNotes ? { id, resolution, notes: trimmedNotes } : { id, resolution });
  }

  function toggleSelect(id: string) {
    setSelectedIds((prev) => {
      const next = new Set(prev);
      if (next.has(id)) {
        next.delete(id);
      } else {
        next.add(id);
      }
      return next;
    });
  }

  function toggleSelectAll() {
    if (selectedIds.size === items.length) {
      setSelectedIds(new Set());
    } else {
      setSelectedIds(new Set(items.map((item) => item.id)));
    }
  }

  const loadingInitial = contradictionsQuery.isLoading && items.length === 0;
  const activeLabel = tabs.find((tab) => tab.value === status)?.label.toLowerCase() ?? status;
  const openItems = items.filter((item) => item.resolution === "open");

  return (
    <div className="mx-auto grid max-w-7xl gap-5">
      <header className="flex flex-col gap-4 sm:flex-row sm:items-center sm:justify-between">
        <div>
          <p className="text-sm font-medium text-accent-strong">Review queue</p>
          <h1 className="mt-1 text-2xl font-semibold tracking-normal text-ink">Contradictions</h1>
        </div>
      </header>

      <div className="flex flex-wrap gap-2" role="tablist" aria-label="Contradiction status">
        {tabs.map((tab) => (
          <button key={tab.value} type="button" role="tab" aria-selected={status === tab.value} onClick={() => selectStatus(tab.value)} className={tabClass(status === tab.value)}>
            {tab.label}
          </button>
        ))}
      </div>

      {contradictionsQuery.isError ? <InlineError message={errorMessage(contradictionsQuery.error)} /> : null}
      {resolveMutation.isError ? <InlineError title="Resolution failed" message={errorMessage(resolveMutation.error)} /> : null}
      {bulkDismissMutation.isError ? <InlineError title="Bulk dismiss failed" message={errorMessage(bulkDismissMutation.error)} /> : null}

      {loadingInitial ? <ContradictionsSkeleton /> : null}

      {!loadingInitial && !contradictionsQuery.isError && items.length === 0 ? (
        <EmptyState title={`No ${activeLabel} contradictions.`} message={`No ${activeLabel} contradictions.`} />
      ) : null}

      {/* Bulk action bar */}
      {status === "open" && selectedIds.size > 0 ? (
        <div className="flex items-center gap-3 rounded-lg border border-amber-200 bg-amber-50 px-4 py-3">
          <span className="text-sm font-medium text-amber-900">{selectedIds.size} selected</span>
          <Button
            type="button"
            variant="secondary"
            size="sm"
            onClick={() => bulkDismissMutation.mutate([...selectedIds])}
            disabled={bulkDismissMutation.isPending}
          >
            {bulkDismissMutation.isPending ? <Loader2 className="h-3.5 w-3.5 animate-spin" aria-hidden="true" /> : <Trash2 className="h-3.5 w-3.5" aria-hidden="true" />}
            Dismiss Selected
          </Button>
          <button type="button" onClick={() => setSelectedIds(new Set())} className="ml-auto text-xs text-amber-700 hover:underline">
            Clear
          </button>
        </div>
      ) : null}

      <div className="grid gap-3">
        {items.map((item) => {
          const isResolved = item.resolution !== "open";
          return (
            <article key={item.id} data-testid={`contradiction-row-${item.id}`} className="rounded-lg border border-line bg-white p-4">
              <div className="flex flex-col gap-3 lg:flex-row lg:items-start lg:justify-between">
                <div className="flex flex-wrap items-center gap-2">
                  {status === "open" ? (
                    <input
                      type="checkbox"
                      aria-label="Select flag"
                      checked={selectedIds.has(item.id)}
                      onChange={() => toggleSelect(item.id)}
                      className="mr-1 h-4 w-4 rounded border-line text-accent accent-accent"
                    />
                  ) : null}
                  <Badge variant={item.conflict_score > 0.5 ? "rust" : "amber"}>Conflict {formatScore(item.conflict_score)}</Badge>
                  <span className="text-xs text-ink/55">Flagged {formatRelativeTime(item.created_at)}</span>
                  <span className="text-xs text-ink/55">Similarity {formatScore(item.similarity)}</span>
                  {isResolved ? <Badge variant="green">{item.resolution.replace("_", " ")}</Badge> : null}
                </div>

                {item.resolution === "open" ? (
                  <div className="relative">
                    <Button
                      type="button"
                      data-testid={`resolve-button-${item.id}`}
                      variant="secondary"
                      size="sm"
                      onClick={() => setResolveTarget(resolveTarget === item.id ? null : item.id)}
                    >
                      <ChevronDown className="h-4 w-4" aria-hidden="true" />
                      Resolve
                    </Button>
                    {resolveTarget === item.id ? (
                      <div className="absolute right-0 z-10 mt-2 w-80 rounded-lg border border-line bg-white p-3 shadow-lg">
                        <textarea
                          value={notes}
                          onChange={(event) => setNotes(event.target.value)}
                          className="min-h-16 w-full rounded-md border border-line px-3 py-2 text-sm outline-none focus:border-accent focus:ring-2 focus:ring-accent/20"
                          placeholder="Notes (optional)"
                        />
                        <div className="mt-3 grid gap-2 sm:grid-cols-2">
                          <Button
                            type="button"
                            size="sm"
                            className="bg-green-600 text-white hover:bg-green-700"
                            onMouseEnter={() => setHoverResolution("keep_a")}
                            onMouseLeave={() => setHoverResolution(null)}
                            onClick={() => submitResolution(item.id, "keep_a")}
                            disabled={resolveMutation.isPending}
                          >
                            {resolveMutation.isPending ? <Loader2 className="h-3.5 w-3.5 animate-spin" aria-hidden="true" /> : <CheckCircle2 className="h-3.5 w-3.5" aria-hidden="true" />}
                            Keep A
                          </Button>
                          <Button
                            type="button"
                            size="sm"
                            className="bg-green-600 text-white hover:bg-green-700"
                            onMouseEnter={() => setHoverResolution("keep_b")}
                            onMouseLeave={() => setHoverResolution(null)}
                            onClick={() => submitResolution(item.id, "keep_b")}
                            disabled={resolveMutation.isPending}
                          >
                            {resolveMutation.isPending ? <Loader2 className="h-3.5 w-3.5 animate-spin" aria-hidden="true" /> : <CheckCircle2 className="h-3.5 w-3.5" aria-hidden="true" />}
                            Keep B
                          </Button>
                          <Button
                            type="button"
                            variant="secondary"
                            size="sm"
                            onClick={() => submitResolution(item.id, "accepted")}
                            disabled={resolveMutation.isPending}
                          >
                            {resolveMutation.isPending ? <Loader2 className="h-3.5 w-3.5 animate-spin" aria-hidden="true" /> : <CheckCircle2 className="h-3.5 w-3.5" aria-hidden="true" />}
                            Accept both
                          </Button>
                          <Button
                            type="button"
                            variant="secondary"
                            size="sm"
                            onClick={() => submitResolution(item.id, "dismissed")}
                            disabled={resolveMutation.isPending}
                          >
                            {resolveMutation.isPending ? <Loader2 className="h-3.5 w-3.5 animate-spin" aria-hidden="true" /> : <XCircle className="h-3.5 w-3.5" aria-hidden="true" />}
                            Dismiss flag
                          </Button>
                        </div>
                      </div>
                    ) : null}
                  </div>
                ) : null}
              </div>

              {/* Winner/loser display for resolved keep_a/keep_b flags */}
              {item.kept_memory_id ? (
                <div className="mt-2 flex flex-wrap gap-2 text-xs">
                  <span className="rounded bg-green-100 px-2 py-0.5 font-medium text-green-800">Winner: {item.kept_memory_id.slice(0, 8)}…</span>
                  <span className="rounded bg-red-50 px-2 py-0.5 text-red-700 line-through">Archived: {item.discarded_memory_id?.slice(0, 8)}…</span>
                </div>
              ) : null}

              <div
                className="mt-4 grid gap-3 lg:grid-cols-2"
                data-hover-resolution={resolveTarget === item.id ? (hoverResolution ?? "") : ""}
              >
                <MemoryPreview
                  title="Memory A"
                  memory={item.memory_a}
                  highlight={resolveTarget === item.id && hoverResolution === "keep_a" ? "keep" : resolveTarget === item.id && hoverResolution === "keep_b" ? "discard" : null}
                />
                <MemoryPreview
                  title="Memory B"
                  memory={item.memory_b}
                  highlight={resolveTarget === item.id && hoverResolution === "keep_b" ? "keep" : resolveTarget === item.id && hoverResolution === "keep_a" ? "discard" : null}
                />
              </div>
            </article>
          );
        })}
      </div>

      {openItems.length > 0 && status === "open" ? (
        <div className="flex items-center gap-3">
          <label className="flex items-center gap-2 text-sm text-ink/70">
            <input
              type="checkbox"
              checked={selectedIds.size === openItems.length && openItems.length > 0}
              onChange={toggleSelectAll}
              className="h-4 w-4 rounded border-line accent-accent"
            />
            Select all visible
          </label>
        </div>
      ) : null}

      {nextCursor ? (
        <div className="flex justify-center">
          <Button type="button" variant="secondary" onClick={() => setAfter(nextCursor)} disabled={contradictionsQuery.isFetching}>
            {contradictionsQuery.isFetching ? <Loader2 className="h-4 w-4 animate-spin" aria-hidden="true" /> : null}
            Load more
          </Button>
        </div>
      ) : null}
    </div>
  );
}

function MemoryPreview({
  title,
  memory,
  highlight,
}: {
  title: string;
  memory: ContradictionItem["memory_a"];
  highlight: "keep" | "discard" | null;
}) {
  return (
    <div
      className={cn(
        "rounded-lg border p-3 transition-all",
        highlight === "keep" && "border-green-400 bg-green-50 ring-2 ring-green-400",
        highlight === "discard" && "border-red-400 bg-red-50 ring-2 ring-red-400",
        !highlight && "border-line bg-soft",
      )}
    >
      <div className="flex items-center justify-between gap-3">
        <p className="text-xs font-semibold uppercase text-ink/45">{title}</p>
        <span className="whitespace-nowrap text-xs text-ink/55">{formatDateTime(memory.created_at)}</span>
      </div>
      <p className="mt-2 text-sm leading-6 text-ink">{memory.content_preview}</p>
    </div>
  );
}

function tabClass(active: boolean): string {
  return cn(
    "inline-flex h-10 items-center rounded-md border px-3 text-sm font-medium transition focus:outline-none focus:ring-2 focus:ring-accent",
    active ? "border-accent bg-accent/10 text-accent-strong" : "border-line bg-white text-ink/70 hover:bg-soft",
  );
}

function ContradictionsSkeleton() {
  return (
    <div className="grid gap-3">
      {Array.from({ length: 4 }, (_, index) => (
        <div key={index} className="rounded-lg border border-line bg-white p-4">
          <div className="flex gap-2">
            <Skeleton className="h-6 w-28" />
            <Skeleton className="h-6 w-36" />
          </div>
          <div className="mt-4 grid gap-3 lg:grid-cols-2">
            <Skeleton className="h-32 w-full" />
            <Skeleton className="h-32 w-full" />
          </div>
        </div>
      ))}
    </div>
  );
}

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : "Contradictions could not be loaded.";
}

function traceLog(_msg: string) {
  // intentionally empty; used as a no-op placeholder for future toast support
}
