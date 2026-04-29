import { CheckCircle2, ChevronDown, Loader2, XCircle } from "lucide-react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useEffect, useState } from "react";

import { listContradictions, resolveContradiction, type ContradictionItem } from "../api/contradictions";
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
  const hasAuth = workspaceId.trim().length > 0 && apiKey.trim().length > 0;

  const contradictionsQuery = useQuery({
    queryKey: ["workspace", workspaceId, "contradictions", status, after],
    queryFn: () => listContradictions(workspaceId, status, after),
    enabled: hasAuth,
  });

  const resolveMutation = useMutation({
    mutationKey: ["workspace", workspaceId, "contradictions", "resolve"],
    mutationFn: ({ id, resolution, notes }: { id: string; resolution: "accepted" | "dismissed"; notes?: string }) =>
      resolveContradiction(workspaceId, id, resolution, notes),
    onSuccess: (resolved) => {
      setItems((current) => current.filter((item) => item.id !== resolved.id));
      setResolveTarget(null);
      setNotes("");
      void queryClient.invalidateQueries({ queryKey: ["workspace", workspaceId, "contradictions"] });
      void queryClient.invalidateQueries({ queryKey: ["workspace", workspaceId, "contradictions", "count"] });
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
  }

  function submitResolution(id: string, resolution: "accepted" | "dismissed") {
    const trimmedNotes = notes.trim();
    resolveMutation.mutate(trimmedNotes ? { id, resolution, notes: trimmedNotes } : { id, resolution });
  }

  const loadingInitial = contradictionsQuery.isLoading && items.length === 0;
  const activeLabel = tabs.find((tab) => tab.value === status)?.label.toLowerCase() ?? status;

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

      {loadingInitial ? <ContradictionsSkeleton /> : null}

      {!loadingInitial && !contradictionsQuery.isError && items.length === 0 ? (
        <EmptyState title={`No ${activeLabel} contradictions.`} message={`No ${activeLabel} contradictions.`} />
      ) : null}

      <div className="grid gap-3">
        {items.map((item) => (
          <article key={item.id} data-testid={`contradiction-row-${item.id}`} className="rounded-lg border border-line bg-white p-4">
            <div className="flex flex-col gap-3 lg:flex-row lg:items-start lg:justify-between">
              <div className="flex flex-wrap items-center gap-2">
                <Badge variant={item.conflict_score > 0.5 ? "rust" : "amber"}>Conflict {formatScore(item.conflict_score)}</Badge>
                <span className="text-xs text-ink/55">Flagged {formatRelativeTime(item.created_at)}</span>
                <span className="text-xs text-ink/55">Similarity {formatScore(item.similarity)}</span>
              </div>
              {item.resolution === "open" ? (
                <div className="relative">
                  <Button type="button" data-testid={`resolve-button-${item.id}`} variant="secondary" size="sm" onClick={() => setResolveTarget(resolveTarget === item.id ? null : item.id)}>
                    <ChevronDown className="h-4 w-4" aria-hidden="true" />
                    Resolve
                  </Button>
                  {resolveTarget === item.id ? (
                    <div className="absolute right-0 z-10 mt-2 w-80 rounded-lg border border-line bg-white p-3 shadow-lg">
                      <textarea value={notes} onChange={(event) => setNotes(event.target.value)} className="min-h-24 w-full rounded-md border border-line px-3 py-2 text-sm outline-none focus:border-accent focus:ring-2 focus:ring-accent/20" placeholder="Notes" />
                      <div className="mt-3 grid gap-2 sm:grid-cols-2">
                        <Button type="button" variant="secondary" size="sm" onClick={() => submitResolution(item.id, "accepted")} disabled={resolveMutation.isPending}>
                          {resolveMutation.isPending ? <Loader2 className="h-3.5 w-3.5 animate-spin" aria-hidden="true" /> : <CheckCircle2 className="h-3.5 w-3.5" aria-hidden="true" />}
                          Accept both
                        </Button>
                        <Button type="button" variant="secondary" size="sm" onClick={() => submitResolution(item.id, "dismissed")} disabled={resolveMutation.isPending}>
                          {resolveMutation.isPending ? <Loader2 className="h-3.5 w-3.5 animate-spin" aria-hidden="true" /> : <XCircle className="h-3.5 w-3.5" aria-hidden="true" />}
                          Dismiss flag
                        </Button>
                      </div>
                    </div>
                  ) : null}
                </div>
              ) : null}
            </div>

            <div className="mt-4 grid gap-3 lg:grid-cols-2">
              <MemoryPreview title="Memory A" memory={item.memory_a} />
              <MemoryPreview title="Memory B" memory={item.memory_b} />
            </div>
          </article>
        ))}
      </div>

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

function MemoryPreview({ title, memory }: { title: string; memory: ContradictionItem["memory_a"] }) {
  return (
    <div className="rounded-lg border border-line bg-soft p-3">
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
