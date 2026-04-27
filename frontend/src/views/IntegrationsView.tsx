import { AlertTriangle, PlugZap, RefreshCw } from "lucide-react";
import { useQuery } from "@tanstack/react-query";

import { listDlq, listIntegrations } from "../api/workspace";
import { EmptyState } from "../components/EmptyState";
import { InlineError } from "../components/InlineError";
import { Badge } from "../components/ui/badge";
import { Button } from "../components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "../components/ui/card";
import { Skeleton } from "../components/ui/skeleton";
import { useAppStore } from "../store/app-store";

export function IntegrationsView() {
  const workspaceId = useAppStore((state) => state.workspaceId);
  const integrations = useQuery({
    queryKey: ["workspace", workspaceId, "integrations"],
    queryFn: () => listIntegrations(workspaceId),
  });
  const dlq = useQuery({
    queryKey: ["workspace", workspaceId, "dlq"],
    queryFn: () => listDlq(workspaceId),
  });

  return (
    <div className="mx-auto grid max-w-7xl gap-5">
      <header className="flex flex-col gap-4 sm:flex-row sm:items-center sm:justify-between">
        <div>
          <p className="text-sm font-medium text-accent-strong">Operations</p>
          <h1 className="mt-1 text-2xl font-semibold tracking-normal text-ink">Integrations</h1>
        </div>
        <Button type="button" variant="secondary" size="sm" onClick={() => void Promise.all([integrations.refetch(), dlq.refetch()])}>
          <RefreshCw className="h-4 w-4" aria-hidden="true" />
          Refresh
        </Button>
      </header>

      {integrations.isError ? <InlineError title="Integrations unavailable" message={integrations.error.message} /> : null}
      {dlq.isError ? <InlineError title="DLQ unavailable" message={dlq.error.message} /> : null}

      <section className="grid gap-4 lg:grid-cols-[1fr_24rem]">
        <Card>
          <CardHeader className="flex flex-row items-center justify-between space-y-0">
            <CardTitle>Sources</CardTitle>
            <PlugZap className="h-4 w-4 text-accent-strong" aria-hidden="true" />
          </CardHeader>
          <CardContent>
            {integrations.isLoading ? <Skeleton className="h-56 w-full" /> : null}
            {!integrations.isLoading && !integrations.isError && (integrations.data?.length ?? 0) === 0 ? (
              <EmptyState title="No integrations" message="Configured sources will be listed here." />
            ) : null}
            {!integrations.isLoading && integrations.data && integrations.data.length > 0 ? (
              <div className="thin-scrollbar overflow-auto rounded-md border border-line">
                <table className="w-full min-w-[620px] border-collapse text-left text-sm">
                  <thead className="bg-soft text-xs uppercase text-ink/55">
                    <tr>
                      <th className="px-3 py-2 font-medium">Source</th>
                      <th className="px-3 py-2 font-medium">Status</th>
                      <th className="px-3 py-2 font-medium">24h events</th>
                      <th className="px-3 py-2 font-medium">24h errors</th>
                      <th className="px-3 py-2 font-medium">Last event</th>
                    </tr>
                  </thead>
                  <tbody>
                    {integrations.data.map((integration) => (
                      <tr key={integration.source} className="border-t border-line">
                        <td className="px-3 py-3 font-medium capitalize">{integration.source}</td>
                        <td className="px-3 py-3"><Badge variant={statusVariant(integration.status)}>{integration.status}</Badge></td>
                        <td className="px-3 py-3">{integration.events_24h}</td>
                        <td className="px-3 py-3">{integration.errors_24h}</td>
                        <td className="px-3 py-3 text-ink/70">{integration.last_event_at ? formatDate(integration.last_event_at) : ""}</td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            ) : null}
          </CardContent>
        </Card>

        <Card>
          <CardHeader className="flex flex-row items-center justify-between space-y-0">
            <CardTitle>DLQ</CardTitle>
            <AlertTriangle className="h-4 w-4 text-rust" aria-hidden="true" />
          </CardHeader>
          <CardContent className="space-y-3">
            {dlq.isLoading ? <Skeleton className="h-40 w-full" /> : null}
            {!dlq.isLoading && !dlq.isError && (dlq.data?.length ?? 0) === 0 ? <EmptyState title="DLQ clear" message="Failed jobs will collect here." /> : null}
            {!dlq.isLoading && dlq.data?.map((entry) => (
              <div key={entry.job_id} className="rounded-md border border-line bg-soft p-3 text-sm">
                <div className="flex items-center justify-between gap-3">
                  <p className="font-mono text-xs text-ink/65">{entry.job_id.slice(0, 8)}</p>
                  <Badge variant="rust">{entry.retry_count}</Badge>
                </div>
                <p className="mt-2 text-rust">{entry.error}</p>
                {entry.payload_summary ? <p className="mt-2 line-clamp-3 font-mono text-xs text-ink/60">{entry.payload_summary}</p> : null}
                {entry.failed_at ? <p className="mt-2 text-xs text-ink/45">{formatDate(entry.failed_at)}</p> : null}
              </div>
            ))}
          </CardContent>
        </Card>
      </section>
    </div>
  );
}

function statusVariant(status: string): "green" | "amber" | "rust" | "muted" {
  if (status === "active") {
    return "green";
  }
  if (status === "degraded") {
    return "amber";
  }
  if (status === "failing") {
    return "rust";
  }
  return "muted";
}

function formatDate(value: string): string {
  const date = new Date(value);
  return Number.isNaN(date.getTime()) ? value : date.toLocaleString();
}
