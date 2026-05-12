import { ArrowRight, BarChart2, BookMarked, CheckCircle2, Database, GitCommit, Pin, Search, Send, Settings2, ShieldAlert, TrendingUp } from "lucide-react";
import { useQuery } from "@tanstack/react-query";
import { useEffect, useState } from "react";
import { Link } from "react-router-dom";

import { getContradictionCount } from "../api/contradictions";
import { InlineError } from "../components/InlineError";
import { MemoryTrendChart } from "../components/MemoryTrendChart";
import { StatusPill } from "../components/StatusPill";
import { Button } from "../components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "../components/ui/card";
import { Skeleton } from "../components/ui/skeleton";
import { HelpTooltip, InfoLabel } from "../components/ui/tooltip";
import { formatCount, formatRelativeTime, formatScore } from "../lib/format";
import { useAppStore } from "../store/app-store";
import { useReadiness } from "../hooks/use-memory";
import { useWorkspaceStats } from "../hooks/use-workspace";

export function Dashboard() {
  const workspaceId = useAppStore((state) => state.workspaceId);
  const readiness = useReadiness(workspaceId);
  const stats = useWorkspaceStats(workspaceId);
  const hasWorkspace = workspaceId.trim().length > 0;
  const [isMounted, setIsMounted] = useState(false);

  useEffect(() => {
    if (!hasWorkspace) {
      setIsMounted(false);
      return;
    }

    const timeoutId = window.setTimeout(() => setIsMounted(true), 500);
    return () => window.clearTimeout(timeoutId);
  }, [hasWorkspace, workspaceId]);

  const contradictionCount = useQuery({
    queryKey: ["workspace", workspaceId, "contradictions", "count"],
    queryFn: () => getContradictionCount(workspaceId),
    enabled: hasWorkspace && isMounted,
    staleTime: 60_000,
    retry: false,
  });

  const readinessStatus = readiness.isLoading
    ? "checking"
    : readiness.data?.status === "ok"
      ? "ready"
      : "unavailable";
  const readinessLabel = readinessStatus === "ready" ? "Backend ready" : readinessStatus === "checking" ? "Checking backend" : "Backend unavailable";

  return (
    <div className="mx-auto grid max-w-7xl gap-6">
      <header className="flex flex-col gap-4 sm:flex-row sm:items-center sm:justify-between">
        <div>
          <p className="text-sm font-medium text-accent-strong">Memory Control Center</p>
          <h1 className="mt-1 text-2xl font-semibold tracking-normal text-ink">Dashboard</h1>
        </div>
        <div className="flex items-center gap-2">
          <StatusPill status={readinessStatus} label={readinessLabel} />
          <HelpTooltip label={readinessLabel}>Shows whether the Control Center can reach the MemoryOps API for this workspace.</HelpTooltip>
        </div>
      </header>

      {readiness.isError ? <InlineError message={errorMessage(readiness.error)} /> : null}
      {stats.isError ? <InlineError title="Stats unavailable" message={errorMessage(stats.error)} /> : null}

      <section className="grid grid-cols-2 gap-4 sm:grid-cols-3 lg:grid-cols-7">
        <MetricCard title="Total memories" helpText="All memories currently stored in this workspace across episodic and semantic units." value={stats.data?.total_memories} loading={stats.isLoading} icon={<Database className="h-4 w-4" />} />
        <MetricCard title="Episodic" helpText="Short-lived event-derived memories from raw activity like commits, PRs, messages, tickets, and agent observations." value={stats.data?.episodic_count} loading={stats.isLoading} icon={<GitCommit className="h-4 w-4" />} />
        <MetricCard title="Semantic" helpText="Durable knowledge promoted from recurring or important episodic memories." value={stats.data?.semantic_count} loading={stats.isLoading} icon={<BookMarked className="h-4 w-4" />} />
        <MetricCard title="Pinned" helpText="Memories protected from normal decay and pruning." value={stats.data?.pinned_count} loading={stats.isLoading} icon={<Pin className="h-4 w-4" />} />
        <MetricCard title="Created 7d" helpText="Memories created in the last seven days so you can gauge how quickly new activity is entering the memory plane." value={stats.data?.memories_created_7d} loading={stats.isLoading} icon={<TrendingUp className="h-4 w-4" />} />
        <ContradictionCountBadge count={contradictionCount.data?.open} loading={contradictionCount.isLoading} />
        <MetricCard
          title="Avg importance"
          helpText="Average priority score used by retrieval, lifecycle, and promotion logic."
          value={stats.data?.avg_importance_score}
          loading={stats.isLoading}
          icon={<BarChart2 className="h-4 w-4" />}
          valueFormatter={formatScore}
        />
      </section>

      <section className="grid gap-4 lg:grid-cols-[1fr_22rem]">
        <div className="grid gap-3">
          <div>
            <p className="text-sm font-medium text-accent-strong">Trend</p>
            <h2 className="mt-1 inline-flex items-center gap-1.5 text-xl font-semibold tracking-normal text-ink">
              <span>Memory Activity (30 days)</span>
              <HelpTooltip label="Memory Activity 30 days">Shows recent memory creation, promotion, and soft-deletion volume across the workspace.</HelpTooltip>
            </h2>
          </div>
          {hasWorkspace && isMounted ? <MemoryTrendChart workspaceId={workspaceId} days={30} /> : null}
        </div>

        <Card>
          <CardHeader>
            <CardTitle className="flex items-center gap-1.5">
              <span>Quick jumps</span>
              <HelpTooltip label="Quick jumps">Fast links into the main operator workflows for exploring, ingesting, promoting, and tracing memory.</HelpTooltip>
            </CardTitle>
          </CardHeader>
          <CardContent className="grid gap-3">
            <Button asChild variant="secondary">
              <Link to="/memory" data-testid="quick-jump-memory" className="justify-between">
                <span className="inline-flex items-center gap-2">
                  <Database className="h-4 w-4" aria-hidden="true" />
                  Explorer
                </span>
                <ArrowRight className="h-4 w-4" aria-hidden="true" />
              </Link>
            </Button>
            <Button asChild variant="secondary">
              <Link to="/ingest" data-testid="quick-jump-ingest" className="justify-between">
                <span className="inline-flex items-center gap-2">
                  <Send className="h-4 w-4" aria-hidden="true" />
                  Webhook Tester
                </span>
                <ArrowRight className="h-4 w-4" aria-hidden="true" />
              </Link>
            </Button>
            <Button asChild variant="secondary">
              <Link to="/lifecycle" data-testid="quick-jump-lifecycle" className="justify-between">
                <span className="inline-flex items-center gap-2">
                  <Settings2 className="h-4 w-4" aria-hidden="true" />
                  Lifecycle
                </span>
                <ArrowRight className="h-4 w-4" aria-hidden="true" />
              </Link>
            </Button>
            <Button asChild variant="secondary">
              <Link to="/trace" data-testid="quick-jump-trace" className="justify-between">
                <span className="inline-flex items-center gap-2">
                  <Search className="h-4 w-4" aria-hidden="true" />
                  Retrieval Trace
                </span>
                <ArrowRight className="h-4 w-4" aria-hidden="true" />
              </Link>
            </Button>
          </CardContent>
        </Card>
      </section>
      <section className="grid gap-4 md:grid-cols-2">
        <Card>
          <CardHeader>
            <CardTitle className="flex items-center gap-1.5">
              <span>Memory health</span>
              <HelpTooltip label="Memory health">Shows decay, deletion, and age signals so you can understand whether the workspace memory pool is stale, noisy, or healthy.</HelpTooltip>
            </CardTitle>
          </CardHeader>
          <CardContent>
            <StatsRows
              loading={stats.isLoading}
              rows={[
                { label: "Avg decay score", helpText: "How strongly a memory is aging out of retrieval. Lower scores are more likely to be pruned or deprioritized.", value: formatScore(stats.data?.avg_decay_score) },
                { label: "Soft-deleted recoverable", helpText: "Memories currently soft-deleted and still recoverable before any hard purge policy removes them.", value: formatCount(stats.data?.deleted_count) },
                { label: "Oldest memory", helpText: "The oldest surviving memory in this workspace, based on its recorded creation time.", value: stats.data?.oldest_memory_at ? formatRelativeTime(stats.data.oldest_memory_at) : "None yet" },
                { label: "Newest memory", helpText: "The most recently created memory now available to retrieval and lifecycle flows.", value: stats.data?.newest_memory_at ? formatRelativeTime(stats.data.newest_memory_at) : "None yet" },
              ]}
            />
          </CardContent>
        </Card>

        <Card>
          <CardHeader>
            <CardTitle className="flex items-center gap-1.5">
              <span>Activity breakdown</span>
              <HelpTooltip label="Activity breakdown">Summarizes how recent activity is distributed across creation volume, durable knowledge, and protected memories.</HelpTooltip>
            </CardTitle>
          </CardHeader>
          <CardContent>
            <StatsRows
              loading={stats.isLoading}
              rows={[
                { label: "Memories created 30d", helpText: "Workspace memories created during the last 30 days.", value: formatCount(stats.data?.memories_created_30d) },
                { label: "Memories created 7d", helpText: "Workspace memories created during the last seven days.", value: formatCount(stats.data?.memories_created_7d) },
                { label: "Semantic ratio", helpText: "Share of the workspace made up of durable semantic memories.", value: formatRatio(stats.data?.semantic_count, stats.data?.total_memories) },
                { label: "Pinned ratio", helpText: "Share of memories protected from normal decay and pruning.", value: formatRatio(stats.data?.pinned_count, stats.data?.total_memories) },
              ]}
            />
          </CardContent>
        </Card>
      </section>
    </div>
  );
}

function ContradictionCountBadge({ count, loading }: { count: number | null | undefined; loading: boolean }) {
  const open = count ?? 0;
  const hasOpen = open > 0;

  return (
    <Link to="/contradictions" data-testid="contradiction-count-badge" className="block rounded-lg focus:outline-none focus:ring-2 focus:ring-accent">
      <Card className={hasOpen ? "border-rust/40 bg-rust/5" : "border-green-200 bg-green-50"}>
        <CardHeader className="flex flex-row items-center justify-between space-y-0 pb-3">
          <CardTitle className="text-sm font-medium text-ink/65">
            <InfoLabel label="Contradictions" tooltip="Potential conflicts detected between memories that may need operator review." />
          </CardTitle>
          <div className={hasOpen ? "text-rust" : "text-green-700"}>
            {hasOpen ? <ShieldAlert className="h-4 w-4" aria-hidden="true" /> : <CheckCircle2 className="h-4 w-4" aria-hidden="true" />}
          </div>
        </CardHeader>
        <CardContent>{loading ? <Skeleton className="h-9 w-16" /> : <p className="text-3xl font-semibold">{formatCount(open)}</p>}</CardContent>
      </Card>
    </Link>
  );
}

export function MetricCard({
  title,
  helpText,
  value,
  loading,
  icon,
  valueFormatter = formatCount,
}: {
  title: string;
  helpText?: string;
  value: number | null | undefined;
  loading: boolean;
  icon: React.ReactNode;
  valueFormatter?: (value: number | null | undefined) => string;
}) {
  return (
    <Card>
      <CardHeader className="flex flex-row items-center justify-between space-y-0 pb-3">
        <CardTitle className="text-sm font-medium text-ink/65">
          {helpText ? <InfoLabel label={title} tooltip={helpText} /> : title}
        </CardTitle>
        <div className="text-accent-strong">{icon}</div>
      </CardHeader>
      <CardContent>
        {loading ? <Skeleton className="h-9 w-24" data-testid="metric-card-skeleton" /> : <p className="text-3xl font-semibold">{valueFormatter(value)}</p>}
      </CardContent>
    </Card>
  );
}

function StatsRows({ loading, rows }: { loading: boolean; rows: Array<{ label: string; value: string; helpText?: string }> }) {
  if (loading) {
    return (
      <div className="grid gap-3">
        {Array.from({ length: 4 }).map((_, index) => (
          <Skeleton className="h-4 w-full" key={index} />
        ))}
      </div>
    );
  }

  return (
    <dl className="grid gap-3">
      {rows.map((row) => (
        <div className="grid grid-cols-[1fr_auto] items-center gap-3" key={row.label}>
          <dt className="text-sm text-ink/60">{row.helpText ? <InfoLabel label={row.label} tooltip={row.helpText} /> : row.label}</dt>
          <dd className="text-sm font-medium text-ink">{row.value}</dd>
        </div>
      ))}
    </dl>
  );
}

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : "The backend did not return a readable response.";
}

function formatRatio(numerator: number | null | undefined, denominator: number | null | undefined): string {
  if (!denominator) {
    return "—";
  }

  return `${(((numerator ?? 0) / denominator) * 100).toFixed(1)}%`;
}
