import { ArrowRight, Loader2, Search } from "lucide-react";
import { useMemo, useState, type CSSProperties, type FormEvent } from "react";

import type { PackedMemory, RetrievalTrace, RetrievalTraceEntry, RetrieveRequest, RetrieveResponse, ScopeFilter, SearchMode, TraceCandidate } from "../api/types";
import { InlineError } from "../components/InlineError";
import { Badge } from "../components/ui/badge";
import { Button } from "../components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "../components/ui/card";
import { Input } from "../components/ui/input";
import { Skeleton } from "../components/ui/skeleton";
import { HelpTooltip, InfoLabel, Tooltip, TooltipContent, TooltipTrigger } from "../components/ui/tooltip";
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
          <CardTitle className="flex items-center gap-1.5">
            <span>Query</span>
            <HelpTooltip label="Query">Run a retrieval request and inspect how MemoryOps ranks and packs memory into agent context.</HelpTooltip>
          </CardTitle>
        </CardHeader>
        <CardContent>
          <form className="grid gap-4" onSubmit={submitQuery}>
            <div className="grid gap-2">
              <label className="text-sm font-medium text-ink" htmlFor="trace-query-input">
                <InfoLabel label="Query" tooltip="Natural-language retrieval request sent into the MemoryOps ranking pipeline." />
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
              <ScopeInput id="trace-agent-id-input" label="Agent ID" helpText="Restrict retrieval to memories visible to a specific agent scope." value={agentId} onChange={setAgentId} />
              <ScopeInput id="trace-user-id-input" label="User ID" helpText="Restrict retrieval to memories visible to a specific user scope." value={userId} onChange={setUserId} />
              <ScopeInput id="trace-repo-input" label="Repo" helpText="Restrict retrieval to memories scoped to a specific repository, usually owner/repo." value={repo} onChange={setRepo} placeholder="owner/repo" />
            </div>
            <p className="text-xs text-ink/55">Leave blank to retrieve across all scopes</p>

            <div className="grid gap-3 md:grid-cols-[minmax(8rem,0.8fr)_minmax(7rem,0.6fr)_minmax(9rem,0.7fr)_auto] md:items-end">
              <div className="grid gap-2">
                <label className="text-sm font-medium text-ink" htmlFor="trace-mode-select">
                  <InfoLabel label="Mode" tooltip="Hybrid combines vector similarity and keyword ranking before final scoring. Vector uses embedding similarity. Keyword uses lexical matching for exact names, IDs, repos, and error strings." />
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
                  <InfoLabel label="Limit" tooltip="Maximum number of candidate memories MemoryOps should consider before final packing." />
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
                  <InfoLabel label="Token budget" tooltip="Maximum approximate context tokens MemoryOps should pack for the agent." />
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

              <Tooltip>
                <TooltipTrigger asChild>
                  <Button type="submit" data-testid="trace-submit" disabled={!canSubmit}>
                    {retrieve.isPending ? <Loader2 className="h-4 w-4 animate-spin" aria-hidden="true" /> : <Search className="h-4 w-4" aria-hidden="true" />}
                    Run query
                  </Button>
                </TooltipTrigger>
                <TooltipContent>Runs the retrieval request and captures the packed results plus the scoring trace.</TooltipContent>
              </Tooltip>
            </div>
          </form>

          {retrieve.isError ? <InlineError title="Retrieval failed" message={errorMessage(retrieve.error)} /> : null}
        </CardContent>
      </Card>

      {retrieve.data ? (
        <section className="grid gap-4">
          <div className="flex flex-wrap items-center gap-3 rounded-md border border-line bg-panel px-4 py-3 text-sm font-medium text-ink" data-testid="trace-summary">
            <SummaryStat label="memories packed" tooltip="How many memories survived filtering and were packed into the final context." value={formatCount(packedItems.length)} />
            <span className="text-ink/35">·</span>
            <SummaryStat label="candidates" tooltip="Candidate memories considered before final inclusion and token-budget filtering." value={formatCount(candidateCount)} />
            <span className="text-ink/35">·</span>
            <SummaryStat label="tokens" tooltip="Approximate total context tokens packed into the final retrieval result." value={formatCount(tokenCount)} />
            <span className="text-ink/35">·</span>
            <SummaryStat label="elapsed ms" tooltip="Approximate end-to-end time the retrieval request took to execute." value={`${formatCount(elapsedMs)}ms`} />
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
          <TraceLabelValue label="RRF" tooltip="Reciprocal Rank Fusion score used to merge ranking signals." value={rrfScore.toFixed(3)} />
          <TraceLabelValue label="Importance" tooltip="Priority score used by retrieval, lifecycle, and promotion logic." value={formatScore(memory.importance_score)} />
          <TraceLabelValue label="Decay" tooltip="How strongly this memory is aging out of retrieval. Lower scores are more likely to be pruned or deprioritized." value={formatScore(memory.decay_score)} />
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
          <div className="flex flex-wrap items-center justify-between gap-3">
            <CardTitle className="flex items-center gap-1.5">
              <span>Trace metadata</span>
              <HelpTooltip label="Trace metadata">Captured request metadata and ranking totals for this retrieval run.</HelpTooltip>
            </CardTitle>
            {trace.feedback_applied ? (
              <Tooltip>
                <TooltipTrigger asChild>
                  <Badge variant="green" tabIndex={0} className="focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent">Feedback applied</Badge>
                </TooltipTrigger>
                <TooltipContent>Memory feedback signals were incorporated into the final retrieval scoring.</TooltipContent>
              </Tooltip>
            ) : null}
          </div>
        </CardHeader>
        <CardContent>
          <dl className="grid gap-3 md:grid-cols-2">
            <TraceMeta label="Query" tooltip="Natural-language retrieval request captured for this trace." value={trace.query_text ?? trace.query ?? "Not recorded"} wide />
            <TraceMeta label="Mode" tooltip="Retrieval mode used for this run." value={trace.search_mode ?? trace.mode ?? "hybrid"} />
            <TraceMeta label="Created" tooltip="When this trace record was captured." value={trace.created_at ? formatDateTime(trace.created_at) : "Not recorded"} />
            <TraceMeta label="Elapsed" tooltip="Approximate execution time for the retrieval request." value={`${formatCount(elapsedMs)}ms`} />
            <TraceMeta label="Candidates" tooltip="Total candidates evaluated before final packing." value={formatCount(totalCandidates)} />
            <TraceMeta label="Included" tooltip="Candidates that survived filtering and were packed into the final context." value={formatCount(trace.included_count)} />
            <TraceMeta label="Token budget" tooltip="Maximum approximate context tokens allowed for this run." value={formatCount(tokenBudget)} />
            <TraceMeta label="Tokens" tooltip="Approximate tokens actually packed into the final result." value={formatCount(tokenCount)} />
          </dl>
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle className="flex items-center gap-1.5">
            <span>Candidates</span>
            <HelpTooltip label="Candidates">Each memory MemoryOps considered, how it scored, and whether it made the final packed context.</HelpTooltip>
          </CardTitle>
        </CardHeader>
        <CardContent>
          <div className="thin-scrollbar overflow-x-auto rounded-md border border-line">
            <table className="w-full min-w-[1120px] border-collapse text-left text-sm" data-testid="trace-candidates-table">
              <thead className="bg-soft text-xs uppercase text-ink/55">
                <tr>
                  <th className="px-3 py-2 font-medium"><InfoLabel label="Memory ID" tooltip="Identifier of the candidate memory evaluated during retrieval." /></th>
                  <th className="px-3 py-2 font-medium"><InfoLabel label="Type" tooltip="Whether the candidate memory is episodic or semantic." /></th>
                  <th className="px-3 py-2 font-medium"><InfoLabel label="Keyword" tooltip="Lexical matching score for exact names, IDs, repos, and strings." /></th>
                  <th className="px-3 py-2 font-medium"><InfoLabel label="Vector" tooltip="Embedding similarity score for semantic retrieval." /></th>
                  <th className="px-3 py-2 font-medium"><InfoLabel label="RRF" tooltip="Reciprocal Rank Fusion score used to merge ranking signals." /></th>
                  <th className="px-3 py-2 font-medium"><InfoLabel label="Relevance" tooltip="Overall relevance score when the backend reports one for the candidate." /></th>
                  <th className="px-3 py-2 font-medium"><InfoLabel label="Decay" tooltip="Aging signal that can lower a memory's retrieval priority over time." /></th>
                  <th className="px-3 py-2 font-medium"><InfoLabel label="Importance" tooltip="Priority score used by retrieval, lifecycle, and promotion logic." /></th>
                  <th className="px-3 py-2 font-medium"><InfoLabel label="Final" tooltip="Final candidate score after combining ranking and memory-control signals." /></th>
                  <th className="px-3 py-2 font-medium"><InfoLabel label="Tokens" tooltip="Approximate token cost of including this memory in context." /></th>
                  <th className="px-3 py-2 font-medium"><InfoLabel label="Included" tooltip="Whether the candidate survived filtering and was packed into the final context." /></th>
                  <th className="px-3 py-2 font-medium"><InfoLabel label="Reason" tooltip="Why the candidate was excluded or any special packing note recorded by the backend." /></th>
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

function TraceMeta({ label, tooltip, value, wide = false }: { label: string; tooltip: string; value: string; wide?: boolean }) {
  return (
    <div className={cn("grid grid-cols-[9rem_1fr] items-start gap-3 border-b border-line/70 pb-3 last:border-b-0 md:last:border-b", wide && "md:col-span-2")}>
      <dt className="text-sm text-ink/60"><InfoLabel label={label} tooltip={tooltip} /></dt>
      <dd className="min-w-0 break-words text-sm font-medium text-ink">{value}</dd>
    </div>
  );
}

function ScopeInput({ id, label, helpText, value, onChange, placeholder }: { id: string; label: string; helpText: string; value: string; onChange: (value: string) => void; placeholder?: string }) {
  return (
    <label className="grid gap-2 text-sm font-medium text-ink" htmlFor={id}>
      <InfoLabel label={label} tooltip={helpText} />
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
      <td className="px-3 py-3">{formatTraceScore(candidate.relevance_score, 2)}</td>
      <td className="px-3 py-3">{formatTraceScore(candidate.decay_score ?? candidate.score_breakdown?.recency)}</td>
      <td className="px-3 py-3">{formatTraceScore(candidate.importance_score ?? candidate.score_breakdown?.importance)}</td>
      <td className="px-3 py-3 font-medium text-ink">{formatTraceScore(candidate.final_score ?? candidate.score)}</td>
      <td className="px-3 py-3">{candidate.token_count === null || candidate.token_count === undefined ? "—" : formatCount(candidate.token_count)}</td>
      <td className="px-3 py-3">
        <span className={cn("font-semibold", candidate.included ? "text-green-700" : "text-orange-700")}>{candidate.included ? "✓" : "✗"}</span>
      </td>
      <td className="max-w-[14rem] px-3 py-3">
        {reason ? (
          <Tooltip>
            <TooltipTrigger asChild>
              <span tabIndex={0} className="inline-block rounded-sm focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent">
                {truncate(reason, 40)}
              </span>
            </TooltipTrigger>
            <TooltipContent>{reason}</TooltipContent>
          </Tooltip>
        ) : "—"}
      </td>
    </tr>
  );
}

function SummaryStat({ label, tooltip, value }: { label: string; tooltip: string; value: string }) {
  return (
    <span className="inline-flex items-center gap-1">
      <InfoLabel label={label} tooltip={tooltip} labelClassName="text-sm font-medium" />
      <span>{value}</span>
    </span>
  );
}

function TraceLabelValue({ label, tooltip, value }: { label: string; tooltip: string; value: string }) {
  return (
    <span className="inline-flex items-center gap-1">
      <InfoLabel label={label} tooltip={tooltip} labelClassName="text-xs text-ink/60" />
      <span>{value}</span>
    </span>
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

function formatTraceScore(value: number | null | undefined, digits = 3): string {
  if (value === null || value === undefined || Number.isNaN(value)) {
    return "—";
  }

  return value.toFixed(digits);
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
