import { CartesianGrid, Legend, Line, LineChart, ResponsiveContainer, Tooltip, XAxis, YAxis } from "recharts";

import { useStatsHistory } from "../hooks/use-workspace";
import { EmptyState } from "./EmptyState";
import { InlineError } from "./InlineError";
import { Card, CardContent } from "./ui/card";
import { Skeleton } from "./ui/skeleton";

type MemoryTrendChartProps = {
  workspaceId: string;
  days?: number;
};

export function MemoryTrendChart({ workspaceId, days = 30 }: MemoryTrendChartProps) {
  const history = useStatsHistory(workspaceId, days);
  const series = history.data?.series ?? [];
  const hasActivity = series.some((point) => point.created > 0 || point.promoted > 0 || point.soft_deleted > 0);

  return (
    <Card>
      <CardContent className="pt-5">
        {history.isPending ? <Skeleton className="h-72 w-full" data-testid="memory-trend-chart-skeleton" /> : null}
        {history.isError ? <InlineError message={errorMessage(history.error)} /> : null}
        {!history.isPending && !history.isError && !hasActivity ? (
          <EmptyState title="No data yet" message="Memory activity will appear here once the workspace has created, promoted, or soft-deleted memories." />
        ) : null}
        {!history.isPending && !history.isError && hasActivity ? (
          <div className="h-72" data-testid="memory-trend-chart">
            <ResponsiveContainer width="100%" height="100%">
              <LineChart data={series} margin={{ top: 12, right: 18, bottom: 4, left: 0 }}>
                <CartesianGrid stroke="#dfe5dc" strokeDasharray="3 3" vertical={false} />
                <XAxis dataKey="date" tickFormatter={formatDateTick} tickMargin={8} minTickGap={18} />
                <YAxis allowDecimals={false} width={36} />
                <Tooltip labelFormatter={(label) => formatDateTick(String(label))} />
                <Legend verticalAlign="bottom" height={32} />
                <Line type="monotone" dataKey="created" name="Created" stroke="#6366f1" strokeWidth={2} dot={false} activeDot={{ r: 4 }} />
                <Line type="monotone" dataKey="promoted" name="Promoted" stroke="#22c55e" strokeWidth={2} dot={false} activeDot={{ r: 4 }} />
                <Line type="monotone" dataKey="soft_deleted" name="Soft Deleted" stroke="#f59e0b" strokeWidth={2} dot={false} activeDot={{ r: 4 }} />
              </LineChart>
            </ResponsiveContainer>
          </div>
        ) : null}
      </CardContent>
    </Card>
  );
}

function formatDateTick(value: string): string {
  const date = new Date(`${value}T00:00:00Z`);
  if (Number.isNaN(date.getTime())) {
    return value;
  }

  return new Intl.DateTimeFormat("en", {
    month: "short",
    day: "numeric",
    timeZone: "UTC",
  }).format(date);
}

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : "Stats history could not be loaded.";
}
