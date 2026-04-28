import { ArrowRight, Loader2, Search } from "lucide-react";
import { useMemo, useState, type CSSProperties, type FormEvent } from "react";

import type { PackedMemory, RetrievalTrace, RetrievalTraceEntry, RetrieveRequest, RetrieveResponse, ScopeFilter, SearchMode, TraceCandidate } from "../api/types";
import { InlineError } from "../components/InlineError";
import { Badge } from "../components/ui/badge";
import { Button } from "../components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "../components/ui/card";
import { Input } from "../components/ui/input";
import { Skeleton } from "../components/ui/skeleton";
import { useRetrieve, useRetrievalTrace } from "../hooks/use-workspace";
import { formatCount, formatDateTime, formatScore } from "../lib/format";
import { cn } from "../lib/utils";
import { useAppStore } from "../store/app-store";

type RetrievalTraceViewProps = {
  initialActiveQueryId?: string;
};

const lineClampStyle = {
  display: "-webkit-box",
  WebkitBoxOrient: "vertical",
  WebkitLineClamp: 3,
  overflow: "hidden",
} satisfies CSSProperties;

export function RetrievalTraceView({ initialActiveQueryId = "" }: RetrievalTraceViewProps = {}) {
  const workspaceId = useAppStore((state) => state.workspaceId);
  const apiKey = useAppStore((state) => state.apiKey);
  const authReady = workspaceId.trim().length > 0 && apiKey.trim().length > 0;
  const retrieve = useRetrieve();
  const [query, setQuery] = useState("");
  const [mode, setMode] = useState<SearchMode>("hybrid");
  const [limit, setLimit] = useState("20");
  const [tokenBudget, setTokenBudget] = useState("4000");
  const [agentId, setAgentId] = useState("");
  const [userId, setUserId] = useState("");
  const [repo, setRepo] = useState("");
  const [activeQueryId, setActiveQueryId] = useState(initialActiveQueryId);
  const [submittedBudget, setSubmittedBudget] = useState(4000);
  const [lastElapsedMs, setLastElapsedMs] = useState<number | null>(null);
  const trace = useRetrievalTrace(activeQueryId);
  const packedItems = useMemo(() => retrieveItems(retrieve.data), [retrieve.data]);
  const tokenCount = retrieveTokenCount(retrieve.data, packedItems);
  const candidateCount = retrieveCandidateCount(retrieve.data, packedItems);
  const elapsedMs = retrieve.data?.elapsed_ms ?? lastElapsedMs ?? 0;
  const canSubmit = query.trim().length > 0 && authReady && !retrieve.isPending;

  function submitQuery(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const trimmed = query.trim();

    if (trimmed.length === 0 || !authReady || retrieve.isPending) {
      return;
    }

    const nextLimit = boundedNumber(limit, 20, 1, 100);
    const nextBudget = boundedNumber(tokenBudget, 4000, 100, 32_000);
    const startedAt = nowMs();

    setLimit(String(nextLimit));
    setTokenBudget(String(nextBudget));
    setSubmittedBudget(nextBudget);
    setActiveQueryId("");
    const scope = buildScopeFilter(agentId, userId, repo);
    const request: RetrieveRequest = {
      query: trimmed,
      limit: nextLimit,
      token_budget: nextBudget,
      mode,
      include_trace: true,
    };

    if (scope !== undefined) {
      request.scope = scope;
    }

    retrieve.mutate(
      request,
      {
        onSuccess: (response) => {
          setLastElapsedMs(response.elapsed_ms ?? Math.max(0, Math.round(nowMs() - startedAt)));
        },
      },
    );
  }

  return (
    <div className="mx-auto grid max-w-7xl gap-6">
      <header>
        <p className="text-sm font-medium text-accent-strong">Retrieval</p>
        <h1 className="mt-1 text-2xl font-semibold tracking-normal text-ink">Retrieval Trace</h1>
        <p className="mt-2 max-w-2xl text-sm text-ink/65">Query the retrieval engine and inspect how each memory was scored and packed.</p>
      </header>

      <Card>
        <CardHeader>
          <CardTitle>Query</CardTitle>
        </CardHeader>
        <CardContent>
          <form className="grid gap-4" onSubmit={submitQuery}>
            <div className="grid gap-2">
              <label className="text-sm font-medium text-ink" htmlFor="trace-query-input">
                Query
              </label>
              <div className="relative">
                <Search className="pointer-events-none absolute left-3 top-3 h-4 w-4 text-ink/40" aria-hidden="true" />
                <textarea
                  id="trace-query-input"
                  data-testid="trace-query-input"
                  className="thin-scrollbar min-h-20 w-full resize-y rounded-md border border-line bg-white px-3 py-2 pl-9 text-sm leading-6 outline-none transition focus:border-accent focus:ring-2 focus:ring-accent/20 disabled:cursor-not-allowed disabled:opacity-50"
                  placeholder="Enter a retrieval query..."
                  value={query}
                  onChange={(event) => setQuery(event.target.value)}
                />
              </div>
            </div>

            <div className="grid gap-3 md:grid-cols-3">
              <ScopeInput id="trace-agent-id-input" label="Agent ID" value={agentId} onChange={setAgentId} />
              <ScopeInput id="trace-user-id-input" label="User ID" value={userId} onChange={setUserId} />
              <ScopeInput id="trace-repo-input" label="Repo" value={repo} onChange={setRepo} placeholder="owner/repo" />
            </div>
            <p className="text-xs text-ink/55">Leave blank to retrieve across all scopes</p>

            <div className="grid gap-3 md:grid-cols-[minmax(8rem,0.8fr)_minmax(7rem,0.6fr)_minmax(9rem,0.7fr)_auto] md:items-end">
              <div className="grid gap-2">
                <label className="text-sm font-medium text-ink" htmlFor="trace-mode-select">
                  Mode
                </label>
                <select
                  id="trace-mode-select"
                  data-testid="trace-mode-select"
                  className="h-10 w-full rounded-md border border-line bg-white px-3 py-2 text-sm outline-none transition focus:border-accent focus:ring-2 focus:ring-accent/20"
                  value={mode}
                  onChange={(event) => setMode(event.target.value as SearchMode)}
                >
                  <option value="hybrid">hybrid</option>
                  <option value="vector">vector</option>
                  <option value="keyword">keyword</option>
                </select>
              </div>

              <div className="grid gap-2">
                <label className="text-sm font-medium text-ink" htmlFor="trace-limit-input">
                  Limit
                </label>
                <Input
                  id="trace-limit-input"
                  data-testid="trace-limit-input"
                  type="number"
                  min={1}
                  max={100}
                  value={limit}
                  onChange={(event) => setLimit(event.target.value)}
                />
              </div>

              <div className="grid gap-2">
                <label className="text-sm font-medium text-ink" htmlFor="trace-budget-input">
                  Token budget
                </label>
                <Input
                  id="trace-budget-input"
                  data-testid="trace-budget-input"
                  type="number"
                  min={100}
                  max={32_000}
                  value={tokenBudget}
                  onChange={(event) => setTokenBudget(event.target.value)}
                />
              </div>

              <Button type="submit" data-testid="trace-submit" disabled={!canSubmit}>
                {retrieve.isPending ? <Loader2 className="h-4 w-4 animate-spin" aria-hidden="true" /> : <Search className="h-4 w-4" aria-hidden="true" />}
                Run query
              </Button>
            </div>
          </form>

          {retrieve.isError ? <InlineError title="Retrieval failed" message={errorMessage(retrieve.error)} /> : null}
        </CardContent>
      </Card>

      {retrieve.data ? (
        <section className="grid gap-4">
          <div className="rounded-md border border-line bg-panel px-4 py-3 text-sm font-medium text-ink" data-testid="trace-summary">
            {formatCount(packedItems.length)} memories packed · {formatCount(candidateCount)} candidates · {formatCount(tokenCount)} tokens · {formatCount(elapsedMs)}ms
          </div>

          <div className="grid gap-3">
            {packedItems.map((memory, index) => (
              <PackedMemoryCard key={memoryKey(memory, index)} memory={memory} index={index} />
            ))}
          </div>

          {packedItems.length > 0 ? (
            <div>
              <Button type="button" variant="secondary" data-testid="trace-view-trace-btn" onClick={() => setActiveQueryId(retrieve.data?.query_id ?? "")}>
                View full trace
                <ArrowRight className="h-4 w-4" aria-hidden="true" />
              </Button>
            </div>
          ) : null}
        </section>
      ) : null}

      {activeQueryId.trim().length > 0 ? (
        <section className="grid gap-4">
          {trace.isLoading ? <TraceSkeleton /> : null}
          {trace.isError ? <InlineError title="Trace unavailable" message={errorMessage(trace.error)} /> : null}
          {trace.data ? <TracePanel trace={trace.data} fallbackBudget={submittedBudget} fallbackElapsedMs={lastElapsedMs} fallbackTokenCount={tokenCount} /> : null}
        </section>
      ) : null}
    </div>
  );
}

function PackedMemoryCard({ memory, index }: { memory: PackedMemory; index: number }) {
  const tags = memory.tags ?? [];
  const rrfScore = memory.rrf_score ?? memory.score_breakdown?.semantic_similarity ?? memory.score_breakdown?.keyword_rank ?? 0;
  const tokenCount = memory.token_count ?? estimateTokens(memory.content);

  return (
    <Card data-testid={`trace-result-${index}`}>
      <CardContent className="grid gap-3 pt-5">
        <div className="flex flex-wrap items-center gap-2">
          <Badge variant={memory.memory_type === "semantic" ? "purple" : "teal"}>{memory.memory_type}</Badge>
          {tags.length > 0 ? <span className="text-xs text-ink/55">{tags.join(", ")}</span> : <span className="text-xs text-ink/45">No tags</span>}
        </div>
        <p className="text-sm leading-6 text-ink" style={lineClampStyle}>
          {memory.content}
        </p>
        <div className="flex flex-wrap gap-x-3 gap-y-1 text-xs text-ink/60">
          <span>RRF {rrfScore.toFixed(3)}</span>
          <span>Importance {formatScore(memory.importance_score)}</span>
          <span>Decay {formatScore(memory.decay_score)}</span>
          <span>{formatCount(tokenCount)} tokens</span>
        </div>
      </CardContent>
    </Card>
  );
}

function TracePanel({
  trace,
  fallbackBudget,
  fallbackElapsedMs,
  fallbackTokenCount,
}: {
  trace: RetrievalTrace;
  fallbackBudget: number;
  fallbackElapsedMs: number | null;
  fallbackTokenCount: number;
}) {
  const candidates = traceCandidates(trace);
  const totalCandidates = trace.total_candidates ?? trace.candidates_evaluated ?? candidates.length;
  const tokenBudget = trace.token_budget ?? fallbackBudget;
  const tokenCount = trace.token_count ?? fallbackTokenCount;
  const elapsedMs = trace.elapsed_ms ?? fallbackElapsedMs ?? 0;

  return (
    <div className="grid gap-4">
      <Card>
        <CardHeader>
          <CardTitle>Trace metadata</CardTitle>
        </CardHeader>
        <CardContent>
          <dl className="grid gap-3 md:grid-cols-2">
            <TraceMeta label="Query" value={trace.query_text ?? trace.query ?? "Not recorded"} wide />
            <TraceMeta label="Mode" value={trace.search_mode ?? trace.mode ?? "hybrid"} />
            <TraceMeta label="Created" value={trace.created_at ? formatDateTime(trace.created_at) : "Not recorded"} />
            <TraceMeta label="Elapsed" value={`${formatCount(elapsedMs)}ms`} />
            <TraceMeta label="Candidates" value={formatCount(totalCandidates)} />
            <TraceMeta label="Included" value={formatCount(trace.included_count)} />
            <TraceMeta label="Token budget" value={formatCount(tokenBudget)} />
            <TraceMeta label="Tokens" value={formatCount(tokenCount)} />
          </dl>
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle>Candidates</CardTitle>
        </CardHeader>
        <CardContent>
          <div className="thin-scrollbar overflow-x-auto rounded-md border border-line">
            <table className="w-full min-w-[1060px] border-collapse text-left text-sm" data-testid="trace-candidates-table">
              <thead className="bg-soft text-xs uppercase text-ink/55">
                <tr>
                  <th className="px-3 py-2 font-medium">Memory ID</th>
                  <th className="px-3 py-2 font-medium">Type</th>
                  <th className="px-3 py-2 font-medium">Keyword</th>
                  <th className="px-3 py-2 font-medium">Vector</th>
                  <th className="px-3 py-2 font-medium">RRF</th>
                  <th className="px-3 py-2 font-medium">Decay</th>
                  <th className="px-3 py-2 font-medium">Importance</th>
                  <th className="px-3 py-2 font-medium">Final</th>
                  <th className="px-3 py-2 font-medium">Tokens</th>
                  <th className="px-3 py-2 font-medium">Included</th>
                  <th className="px-3 py-2 font-medium">Reason</th>
                </tr>
              </thead>
              <tbody>
                {candidates.map((candidate) => (
                  <TraceCandidateRow key={`${candidate.memory_id}:${candidate.included}:${candidate.exclusion_reason ?? "included"}`} candidate={candidate} />
                ))}
              </tbody>
            </table>
          </div>
        </CardContent>
      </Card>
    </div>
  );
}

function TraceMeta({ label, value, wide = false }: { label: string; value: string; wide?: boolean }) {
  return (
    <div className={cn("grid grid-cols-[9rem_1fr] items-start gap-3 border-b border-line/70 pb-3 last:border-b-0 md:last:border-b", wide && "md:col-span-2")}>
      <dt className="text-sm text-ink/60">{label}</dt>
      <dd className="min-w-0 break-words text-sm font-medium text-ink">{value}</dd>
    </div>
  );
}

function ScopeInput({ id, label, value, onChange, placeholder }: { id: string; label: string; value: string; onChange: (value: string) => void; placeholder?: string }) {
  return (
    <label className="grid gap-2 text-sm font-medium text-ink" htmlFor={id}>
      {label}
      <Input id={id} value={value} onChange={(event) => onChange(event.target.value)} placeholder={placeholder} />
    </label>
  );
}

function TraceCandidateRow({ candidate }: { candidate: RetrievalTraceEntry }) {
  const reason = candidate.exclusion_reason ?? "";

  return (
    <tr className={cn("border-t border-line align-top", !candidate.included && "bg-soft/70 text-ink/55")}>
      <td className="whitespace-nowrap px-3 py-3 font-mono text-xs">{shortId(candidate.memory_id)}</td>
      <td className="px-3 py-3">{candidate.memory_type ?? "—"}</td>
      <td className="px-3 py-3">{formatTraceScore(candidate.keyword_score ?? candidate.score_breakdown?.keyword_rank)}</td>
      <td className="px-3 py-3">{formatTraceScore(candidate.vector_score ?? candidate.score_breakdown?.semantic_similarity)}</td>
      <td className="px-3 py-3">{formatTraceScore(candidate.rrf_score ?? candidate.score)}</td>
      <td className="px-3 py-3">{formatTraceScore(candidate.decay_score ?? candidate.score_breakdown?.recency)}</td>
      <td className="px-3 py-3">{formatTraceScore(candidate.importance_score ?? candidate.score_breakdown?.importance)}</td>
      <td className="px-3 py-3 font-medium text-ink">{formatTraceScore(candidate.final_score ?? candidate.score)}</td>
      <td className="px-3 py-3">{candidate.token_count === null || candidate.token_count === undefined ? "—" : formatCount(candidate.token_count)}</td>
      <td className="px-3 py-3">
        <span className={cn("font-semibold", candidate.included ? "text-green-700" : "text-orange-700")}>{candidate.included ? "✓" : "✗"}</span>
      </td>
      <td className="max-w-[14rem] px-3 py-3" title={reason || undefined}>
        {reason ? truncate(reason, 40) : "—"}
      </td>
    </tr>
  );
}

function TraceSkeleton() {
  return (
    <Card>
      <CardContent className="grid gap-3 pt-5">
        {Array.from({ length: 5 }).map((_, index) => (
          <Skeleton key={index} className="h-10 w-full" />
        ))}
      </CardContent>
    </Card>
  );
}

function retrieveItems(response: RetrieveResponse | undefined): PackedMemory[] {
  return response?.items ?? response?.memories ?? [];
}

function retrieveTokenCount(response: RetrieveResponse | undefined, items: PackedMemory[]): number {
  return response?.token_count ?? response?.total_tokens ?? items.reduce((total, item) => total + (item.token_count ?? estimateTokens(item.content)), 0);
}

function retrieveCandidateCount(response: RetrieveResponse | undefined, items: PackedMemory[]): number {
  return response?.total_candidates ?? response?.trace?.total_candidates ?? response?.trace?.candidates_evaluated ?? items.length;
}

function traceCandidates(trace: RetrievalTrace): TraceCandidate[] {
  return trace.candidates ?? trace.entries ?? [];
}

function buildScopeFilter(agentId: string, userId: string, repo: string): ScopeFilter | undefined {
  const scope: ScopeFilter = {};
  const normalizedAgentId = optionalText(agentId);
  const normalizedUserId = optionalText(userId);
  const normalizedRepo = optionalText(repo);

  if (normalizedAgentId !== undefined) {
    scope.agent_id = normalizedAgentId;
  }
  if (normalizedUserId !== undefined) {
    scope.user_id = normalizedUserId;
  }
  if (normalizedRepo !== undefined) {
    scope.repo = normalizedRepo;
  }

  return Object.keys(scope).length > 0 ? scope : undefined;
}

function optionalText(value: string): string | undefined {
  const trimmed = value.trim();
  return trimmed.length > 0 ? trimmed : undefined;
}

function boundedNumber(value: string, fallback: number, min: number, max: number): number {
  const parsed = Number(value);
  if (!Number.isFinite(parsed)) {
    return fallback;
  }

  return Math.min(Math.max(Math.round(parsed), min), max);
}

function estimateTokens(content: string): number {
  return Math.max(1, Math.floor(content.length / 4));
}

function formatTraceScore(value: number | null | undefined): string {
  if (value === null || value === undefined || Number.isNaN(value)) {
    return "—";
  }

  return value.toFixed(3);
}

function memoryKey(memory: PackedMemory, index: number): string {
  return memory.memory_id ?? memory.id ?? `packed-${index}`;
}

function shortId(value: string): string {
  return value.slice(0, 8);
}

function truncate(value: string, length: number): string {
  if (value.length <= length) {
    return value;
  }

  return `${value.slice(0, length - 1)}...`;
}

function nowMs(): number {
  return typeof performance === "undefined" ? Date.now() : performance.now();
}

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : "The backend did not return a readable response.";
}
