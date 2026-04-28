import { ArrowRight, BarChart2, BookMarked, Database, GitCommit, Pin, Search, Send, Settings2, TrendingUp } from "lucide-react";
import { useMemo } from "react";
import { Link } from "react-router-dom";
import { Line, LineChart, ResponsiveContainer, Tooltip } from "recharts";

import type { MemoryUnit } from "../api/types";
import { EmptyState } from "../components/EmptyState";
import { InlineError } from "../components/InlineError";
import { StatusPill } from "../components/StatusPill";
import { Button } from "../components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "../components/ui/card";
import { Skeleton } from "../components/ui/skeleton";
import { dayKey, formatCount, formatRelativeTime, formatScore } from "../lib/format";
import { useAppStore } from "../store/app-store";
import { useMemoryList, useReadiness } from "../hooks/use-memory";
import { useWorkspaceStats } from "../hooks/use-workspace";

type SparklinePoint = {
  day: string;
  count: number;
};

export function Dashboard() {
  const workspaceId = useAppStore((state) => state.workspaceId);
  const readiness = useReadiness(workspaceId);
  const stats = useWorkspaceStats(workspaceId);
  const recent = useMemoryList(workspaceId, { limit: 100, offset: 0, sort: "created_at", direction: "asc" });
  const sparklineData = useMemo(() => buildSparklineData(recent.data?.items ?? []), [recent.data?.items]);

  const readinessStatus = readiness.isLoading
    ? "checking"
    : readiness.data?.status === "ok"
      ? "ready"
      : "unavailable";

  return (
    <div className="mx-auto grid max-w-7xl gap-6">
      <header className="flex flex-col gap-4 sm:flex-row sm:items-center sm:justify-between">
        <div>
          <p className="text-sm font-medium text-accent-strong">Memory Control Center</p>
          <h1 className="mt-1 text-2xl font-semibold tracking-normal text-ink">Dashboard</h1>
        </div>
        <StatusPill
          status={readinessStatus}
          label={readinessStatus === "ready" ? "Backend ready" : readinessStatus === "checking" ? "Checking backend" : "Backend unavailable"}
        />
      </header>

      {readiness.isError ? <InlineError message={errorMessage(readiness.error)} /> : null}
      {stats.isError ? <InlineError title="Stats unavailable" message={errorMessage(stats.error)} /> : null}

      <section className="grid grid-cols-2 gap-4 sm:grid-cols-3 lg:grid-cols-6">
        <MetricCard title="Total" value={stats.data?.total_memories} loading={stats.isLoading} icon={<Database className="h-4 w-4" />} />
        <MetricCard title="Episodic" value={stats.data?.episodic_count} loading={stats.isLoading} icon={<GitCommit className="h-4 w-4" />} />
        <MetricCard title="Semantic" value={stats.data?.semantic_count} loading={stats.isLoading} icon={<BookMarked className="h-4 w-4" />} />
        <MetricCard title="Pinned" value={stats.data?.pinned_count} loading={stats.isLoading} icon={<Pin className="h-4 w-4" />} />
        <MetricCard title="Created (7d)" value={stats.data?.memories_created_7d} loading={stats.isLoading} icon={<TrendingUp className="h-4 w-4" />} />
        <MetricCard
          title="Avg importance"
          value={stats.data?.avg_importance_score}
          loading={stats.isLoading}
          icon={<BarChart2 className="h-4 w-4" />}
          valueFormatter={formatScore}
        />
      </section>

      <section className="grid gap-4 lg:grid-cols-[1fr_22rem]">
        <Card>
          <CardHeader>
            <CardTitle>Memory created over time</CardTitle>
          </CardHeader>
          <CardContent>
            {recent.isLoading ? <Skeleton className="h-44 w-full" /> : null}
            {recent.isError ? <InlineError message={errorMessage(recent.error)} /> : null}
            {!recent.isLoading && !recent.isError && sparklineData.length === 0 ? (
              <EmptyState title="The timeline is ready" message="Fresh memories will draw the first shape here after ingestion starts." />
            ) : null}
            {!recent.isLoading && !recent.isError && sparklineData.length > 0 ? (
              <div className="h-44" data-testid="dashboard-sparkline">
                <ResponsiveContainer width="100%" height="100%">
                  <LineChart data={sparklineData} margin={{ top: 10, right: 14, bottom: 6, left: 0 }}>
                    <Tooltip labelFormatter={(label) => `Created ${label}`} formatter={(value) => [value, "Memories"]} />
                    <Line type="monotone" dataKey="count" stroke="#19736a" strokeWidth={2} dot={false} activeDot={{ r: 4 }} />
                  </LineChart>
                </ResponsiveContainer>
              </div>
            ) : null}
          </CardContent>
        </Card>

        <Card>
          <CardHeader>
            <CardTitle>Quick jumps</CardTitle>
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
            <CardTitle>Memory health</CardTitle>
          </CardHeader>
          <CardContent>
            <StatsRows
              loading={stats.isLoading}
              rows={[
                { label: "Avg decay score", value: formatScore(stats.data?.avg_decay_score) },
                { label: "Soft-deleted (recoverable)", value: formatCount(stats.data?.deleted_count) },
                { label: "Oldest memory", value: stats.data?.oldest_memory_at ? formatRelativeTime(stats.data.oldest_memory_at) : "None yet" },
                { label: "Newest memory", value: stats.data?.newest_memory_at ? formatRelativeTime(stats.data.newest_memory_at) : "None yet" },
              ]}
            />
          </CardContent>
        </Card>

        <Card>
          <CardHeader>
            <CardTitle>30-day activity</CardTitle>
          </CardHeader>
          <CardContent>
            <StatsRows
              loading={stats.isLoading}
              rows={[
                { label: "Memories created (30d)", value: formatCount(stats.data?.memories_created_30d) },
                { label: "Memories created (7d)", value: formatCount(stats.data?.memories_created_7d) },
                { label: "Semantic ratio", value: formatRatio(stats.data?.semantic_count, stats.data?.total_memories) },
                { label: "Pinned ratio", value: formatRatio(stats.data?.pinned_count, stats.data?.total_memories) },
              ]}
            />
          </CardContent>
        </Card>
      </section>
    </div>
  );
}

export function MetricCard({
  title,
  value,
  loading,
  icon,
  valueFormatter = formatCount,
}: {
  title: string;
  value: number | null | undefined;
  loading: boolean;
  icon: React.ReactNode;
  valueFormatter?: (value: number | null | undefined) => string;
}) {
  return (
    <Card>
      <CardHeader className="flex flex-row items-center justify-between space-y-0 pb-3">
        <CardTitle className="text-sm font-medium text-ink/65">{title}</CardTitle>
        <div className="text-accent-strong">{icon}</div>
      </CardHeader>
      <CardContent>
        {loading ? <Skeleton className="h-9 w-24" data-testid="metric-card-skeleton" /> : <p className="text-3xl font-semibold">{valueFormatter(value)}</p>}
      </CardContent>
    </Card>
  );
}

function StatsRows({ loading, rows }: { loading: boolean; rows: Array<{ label: string; value: string }> }) {
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
          <dt className="text-sm text-ink/60">{row.label}</dt>
          <dd className="text-sm font-medium text-ink">{row.value}</dd>
        </div>
      ))}
    </dl>
  );
}

function buildSparklineData(items: MemoryUnit[]): SparklinePoint[] {
  const counts = new Map<string, number>();
  items.forEach((item) => {
    const key = dayKey(item.created_at);
    counts.set(key, (counts.get(key) ?? 0) + 1);
  });

  return Array.from(counts.entries()).map(([day, count]) => ({ day, count }));
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
