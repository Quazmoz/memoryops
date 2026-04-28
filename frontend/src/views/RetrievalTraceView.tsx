import { ChevronDown, Loader2, Search } from "lucide-react";
import { useMutation, useQuery } from "@tanstack/react-query";
import { useMemo, useState, type FormEvent } from "react";

import { getRetrievalTrace, retrieveMemory } from "../api/memory";
import type { MemoryType, MemoryUnit, PackedMemory, RetrievalTrace, RetrievalTraceEntry } from "../api/types";
import { EmptyState } from "../components/EmptyState";
import { InlineError } from "../components/InlineError";
import { MemoryResultsTable, type MemoryRow } from "../components/MemoryResultsTable";
import { Badge } from "../components/ui/badge";
import { Button } from "../components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "../components/ui/card";
import { Input } from "../components/ui/input";
import { Skeleton } from "../components/ui/skeleton";
import { formatCount, formatScore } from "../lib/format";
import { useAppStore } from "../store/app-store";

export function RetrievalTraceView() {
  const workspaceId = useAppStore((state) => state.workspaceId);
  const apiKey = useAppStore((state) => state.apiKey);
  const authReady = workspaceId.trim().length > 0 && apiKey.trim().length > 0;
  const [query, setQuery] = useState("");
  const [submittedQuery, setSubmittedQuery] = useState("");
  const [traceOpen, setTraceOpen] = useState(false);
  const [queryId, setQueryId] = useState<string | null>(null);
  const retrieve = useMutation({
    mutationKey: ["workspace", workspaceId, "retrieve"],
    mutationFn: (nextQuery: string) =>
      retrieveMemory({
        query: nextQuery,
        workspace_id: workspaceId,
        mode: "hybrid",
        include_trace: false,
      }),
    onSuccess: (response, nextQuery) => {
      setSubmittedQuery(nextQuery);
      setQueryId(response.query_id);
      setTraceOpen(false);
    },
  });
  const trace = useQuery({
    queryKey: ["workspace", workspaceId, "retrieve-trace", queryId],
    queryFn: () => getRetrievalTrace(workspaceId, queryId ?? ""),
    enabled: authReady && traceOpen && Boolean(queryId),
  });
  const rows = useMemo(
    () => (retrieve.data?.memories ?? []).map((memory, index) => packedMemoryToRow(memory, workspaceId, index + 1)),
    [retrieve.data?.memories, workspaceId],
  );

  function submitSearch(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const trimmed = query.trim();
    if (trimmed.length === 0 || !authReady) {
      return;
    }
    retrieve.mutate(trimmed);
  }

  return (
    <div className="mx-auto grid max-w-7xl gap-5">
      <header className="flex flex-col gap-4 xl:flex-row xl:items-end xl:justify-between">
        <div>
          <p className="text-sm font-medium text-accent-strong">Retrieval</p>
          <h1 className="mt-1 text-2xl font-semibold tracking-normal text-ink">Retrieval Trace</h1>
        </div>

        <form className="flex w-full max-w-2xl gap-2" onSubmit={submitSearch}>
          <div className="relative flex-1">
            <Search className="pointer-events-none absolute left-3 top-1/2 h-4 w-4 -translate-y-1/2 text-ink/40" aria-hidden="true" />
            <Input value={query} onChange={(event) => setQuery(event.target.value)} className="pl-9" placeholder="Search memory" />
          </div>
          <Button type="submit" disabled={!authReady || retrieve.isPending || query.trim().length === 0}>
            {retrieve.isPending ? <Loader2 className="h-4 w-4 animate-spin" aria-hidden="true" /> : <Search className="h-4 w-4" aria-hidden="true" />}
            Search
          </Button>
        </form>
      </header>

      {retrieve.isError ? <InlineError title="Retrieval failed" message={retrieve.error.message} /> : null}
      {submittedQuery ? (
        <div className="flex flex-wrap items-center gap-2 text-sm text-ink/65">
          <span>Query</span>
          <Badge variant="accent">{submittedQuery}</Badge>
          {retrieve.data ? <Badge variant="muted">{formatCount(retrieve.data.total_tokens)} tokens</Badge> : null}
        </div>
      ) : null}

      {retrieve.isPending || rows.length > 0 ? (
        <MemoryResultsTable rows={rows} loading={retrieve.isPending} pendingMemoryIds={[]} showPinControls={false} />
      ) : null}
      {!retrieve.isPending && retrieve.data && rows.length === 0 ? <EmptyState title="No memories returned" message="The retrieval query completed without packed memory results." /> : null}

      {retrieve.data ? (
        <Card>
          <CardHeader className="p-0">
            <button
              type="button"
              className="flex w-full items-center justify-between gap-3 p-5 text-left"
              onClick={() => setTraceOpen((open) => !open)}
              aria-expanded={traceOpen}
            >
              <CardTitle>View trace</CardTitle>
              <ChevronDown className={`h-4 w-4 text-accent-strong transition ${traceOpen ? "rotate-180" : ""}`} aria-hidden="true" />
            </button>
          </CardHeader>
          {traceOpen ? (
            <CardContent>
              {trace.isLoading ? <Skeleton className="h-56 w-full" /> : null}
              {trace.isError ? <InlineError title="Trace unavailable" message={trace.error.message} /> : null}
              {trace.data ? <TracePanel trace={trace.data} /> : null}
            </CardContent>
          ) : null}
        </Card>
      ) : (
        <EmptyState title="Run a retrieval query to see the trace" message="Trace details will appear below the retrieval results." />
      )}
    </div>
  );
}

function TracePanel({ trace }: { trace: RetrievalTrace }) {
  const excluded = trace.entries.filter((entry) => !entry.included);

  return (
    <div className="grid gap-4">
      <div className="grid gap-3 sm:grid-cols-3">
        <TraceMetric label="candidates_evaluated" value={trace.candidates_evaluated} />
        <TraceMetric label="included_count" value={trace.included_count} />
        <TraceMetric label="excluded_count" value={trace.excluded_count} />
      </div>

      <div className="thin-scrollbar overflow-auto rounded-md border border-line">
        <table className="w-full min-w-[880px] border-collapse text-left text-sm">
          <thead className="bg-soft text-xs uppercase text-ink/55">
            <tr>
              <th className="px-3 py-2 font-medium">Memory ID</th>
              <th className="px-3 py-2 font-medium">vector_score</th>
              <th className="px-3 py-2 font-medium">keyword_score</th>
              <th className="px-3 py-2 font-medium">rrf_score</th>
              <th className="px-3 py-2 font-medium">decay_score</th>
              <th className="px-3 py-2 font-medium">final_score</th>
              <th className="px-3 py-2 font-medium">Status</th>
            </tr>
          </thead>
          <tbody>
            {trace.entries.map((entry) => (
              <TraceRow key={`${entry.memory_id}:${entry.included}:${entry.exclusion_reason ?? "included"}`} entry={entry} />
            ))}
          </tbody>
        </table>
      </div>

      <div className="rounded-md border border-line bg-soft p-4">
        <p className="text-sm font-semibold text-ink">Exclusion reasons</p>
        {excluded.length > 0 ? (
          <div className="mt-3 grid gap-2">
            {excluded.map((entry) => (
              <div key={`${entry.memory_id}:${entry.exclusion_reason ?? "excluded"}`} className="flex flex-wrap items-center gap-2 text-sm text-ink/70">
                <span className="font-mono text-xs">{shortId(entry.memory_id)}</span>
                <Badge variant="amber">{entry.exclusion_reason ?? "excluded"}</Badge>
              </div>
            ))}
          </div>
        ) : (
          <p className="mt-2 text-sm text-ink/60">No excluded results.</p>
        )}
      </div>
    </div>
  );
}

function TraceMetric({ label, value }: { label: string; value: number }) {
  return (
    <div className="rounded-md border border-line bg-soft px-3 py-2">
      <p className="text-xs font-medium uppercase text-ink/45">{label}</p>
      <p className="mt-1 text-lg font-semibold text-ink">{formatCount(value)}</p>
    </div>
  );
}

function TraceRow({ entry }: { entry: RetrievalTraceEntry }) {
  return (
    <tr className="border-t border-line align-top">
      <td className="whitespace-nowrap px-3 py-3 font-mono text-xs text-ink/70">{shortId(entry.memory_id)}</td>
      <td className="px-3 py-3">{formatScore(entry.score_breakdown.semantic_similarity)}</td>
      <td className="px-3 py-3">{formatScore(entry.score_breakdown.keyword_rank)}</td>
      <td className="px-3 py-3">{formatScore(entry.score)}</td>
      <td className="px-3 py-3">{formatScore(entry.score_breakdown.recency)}</td>
      <td className="px-3 py-3 font-semibold text-ink">{formatScore(entry.score)}</td>
      <td className="px-3 py-3">
        <Badge variant={entry.included ? "green" : "amber"}>{entry.included ? "included" : "excluded"}</Badge>
      </td>
    </tr>
  );
}

function packedMemoryToRow(memory: PackedMemory, workspaceId: string, rank: number): MemoryRow {
  const memoryType: MemoryType = memory.memory_type === "semantic" ? "semantic" : "episodic";

  return {
    id: memory.id,
    workspace_id: workspaceId,
    scope: { workspace_id: workspaceId },
    memory_type: memoryType,
    content: memory.content,
    entities: memory.entities,
    importance_score: memory.importance_score,
    decay_score: memory.decay_score,
    pinned: false,
    tags: [],
    source_events: [],
    source_episode_ids: [],
    corroboration_count: 1,
    created_at: "",
    updated_at: "",
    rank,
  };
}

function shortId(value: string): string {
  if (value.length <= 13) {
    return value;
  }
  return `${value.slice(0, 8)}...${value.slice(-4)}`;
}
