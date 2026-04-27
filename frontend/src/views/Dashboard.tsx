import { ArrowRight, Database, GitPullRequest, Send } from "lucide-react";
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
import { dayKey, formatCount } from "../lib/format";
import { useAppStore } from "../store/app-store";
import { useMemoryList, useReadiness } from "../hooks/use-memory";

type SparklinePoint = {
  day: string;
  count: number;
};

export function Dashboard() {
  const workspaceId = useAppStore((state) => state.workspaceId);
  const readiness = useReadiness(workspaceId);
  const episodic = useMemoryList(workspaceId, { limit: 1, offset: 0, memoryType: "episodic" });
  const semantic = useMemoryList(workspaceId, { limit: 1, offset: 0, memoryType: "semantic" });
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
          <p className="text-sm font-medium text-accent-strong">M5 Memory Control Center</p>
          <h1 className="mt-1 text-2xl font-semibold tracking-normal text-ink">Dashboard</h1>
        </div>
        <StatusPill
          status={readinessStatus}
          label={readinessStatus === "ready" ? "Backend ready" : readinessStatus === "checking" ? "Checking backend" : "Backend unavailable"}
        />
      </header>

      {readiness.isError ? <InlineError message={errorMessage(readiness.error)} /> : null}
      {episodic.isError || semantic.isError ? (
        <InlineError title="Memory counts unavailable" message={errorMessage(episodic.error ?? semantic.error)} />
      ) : null}

      <section className="grid gap-4 md:grid-cols-3">
        <MetricCard title="Episodic memory" value={episodic.data?.total} loading={episodic.isLoading} icon={<GitPullRequest className="h-4 w-4" />} />
        <MetricCard title="Semantic memory" value={semantic.data?.total} loading={semantic.isLoading} icon={<Database className="h-4 w-4" />} />
        <MetricCard title="Total memory" value={(episodic.data?.total ?? 0) + (semantic.data?.total ?? 0)} loading={episodic.isLoading || semantic.isLoading} icon={<Database className="h-4 w-4" />} />
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
          </CardContent>
        </Card>
      </section>
    </div>
  );
}

function MetricCard({ title, value, loading, icon }: { title: string; value: number | undefined; loading: boolean; icon: React.ReactNode }) {
  return (
    <Card>
      <CardHeader className="flex flex-row items-center justify-between space-y-0 pb-3">
        <CardTitle className="text-sm font-medium text-ink/65">{title}</CardTitle>
        <div className="text-accent-strong">{icon}</div>
      </CardHeader>
      <CardContent>
        {loading ? <Skeleton className="h-9 w-24" /> : <p className="text-3xl font-semibold">{formatCount(value)}</p>}
      </CardContent>
    </Card>
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
