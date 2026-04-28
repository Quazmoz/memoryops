import { AlertTriangle, PlugZap, RefreshCw, RotateCcw, Trash2 } from "lucide-react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";

import { discardDlqJob, listDlq, listIntegrations, retryDlqJob } from "../api/integrations";
import type { DlqEntry } from "../api/types";
import { EmptyState } from "../components/EmptyState";
import { InlineError } from "../components/InlineError";
import { Badge } from "../components/ui/badge";
import { Button } from "../components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "../components/ui/card";
import { Skeleton } from "../components/ui/skeleton";
import { formatCount, formatDateTime } from "../lib/format";
import { cn } from "../lib/utils";
import { useAppStore } from "../store/app-store";

export function IntegrationsView() {
  const workspaceId = useAppStore((state) => state.workspaceId);
  const queryClient = useQueryClient();
  const integrationsQueryKey = ["workspace", workspaceId, "integrations"];
  const dlqQueryKey = ["workspace", workspaceId, "dlq"];
  const integrations = useQuery({
    queryKey: integrationsQueryKey,
    queryFn: () => listIntegrations(workspaceId),
    enabled: workspaceId.trim().length > 0,
  });
  const dlq = useQuery({
    queryKey: dlqQueryKey,
    queryFn: () => listDlq(workspaceId),
    enabled: workspaceId.trim().length > 0,
  });
  const retryMutation = useMutation({
    mutationFn: (jobId: string) => retryDlqJob(workspaceId, jobId),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: dlqQueryKey });
    },
  });
  const discardMutation = useMutation<void, Error, string, { previous: DlqEntry[] | undefined }>({
    mutationFn: (jobId) => discardDlqJob(workspaceId, jobId),
    onMutate: async (jobId) => {
      await queryClient.cancelQueries({ queryKey: dlqQueryKey });
      const previous = queryClient.getQueryData<DlqEntry[]>(dlqQueryKey);
      queryClient.setQueryData<DlqEntry[]>(dlqQueryKey, (current) => current?.filter((entry) => entry.job_id !== jobId) ?? []);
      return { previous };
    },
    onError: (_error, _jobId, context) => {
      if (context?.previous) {
        queryClient.setQueryData(dlqQueryKey, context.previous);
      }
    },
    onSettled: () => {
      void queryClient.invalidateQueries({ queryKey: dlqQueryKey });
    },
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
      {retryMutation.isError ? <InlineError title="Retry failed" message={retryMutation.error.message} /> : null}
      {discardMutation.isError ? <InlineError title="Discard failed" message={discardMutation.error.message} /> : null}

      <Card>
        <CardHeader className="flex flex-row items-center justify-between space-y-0">
          <CardTitle>Integration Health</CardTitle>
          <PlugZap className="h-4 w-4 text-accent-strong" aria-hidden="true" />
        </CardHeader>
        <CardContent>
          {integrations.isLoading ? <Skeleton className="h-56 w-full" /> : null}
          {!integrations.isLoading && !integrations.isError && (integrations.data?.length ?? 0) === 0 ? (
            <EmptyState title="No integrations configured" message="No integrations configured — add a GitHub webhook to get started." />
          ) : null}
          {!integrations.isLoading && integrations.data && integrations.data.length > 0 ? (
            <div className="thin-scrollbar overflow-auto rounded-md border border-line">
              <table className="w-full min-w-[720px] border-collapse text-left text-sm">
                <thead className="bg-soft text-xs uppercase text-ink/55">
                  <tr>
                    <th className="px-3 py-2 font-medium">Source</th>
                    <th className="px-3 py-2 font-medium">Status</th>
                    <th className="px-3 py-2 font-medium">Last Event</th>
                    <th className="px-3 py-2 font-medium">Events 24h</th>
                    <th className="px-3 py-2 font-medium">Errors 24h</th>
                  </tr>
                </thead>
                <tbody>
                  {integrations.data.map((integration) => (
                    <tr key={integration.source} className="border-t border-line">
                      <td className="px-3 py-3">
                        <Badge variant="muted" className="capitalize">{integration.source}</Badge>
                      </td>
                      <td className="px-3 py-3">
                        <div className="flex items-center gap-2">
                          <span className={cn("h-2.5 w-2.5 rounded-full", statusDotClass(integration.status))} aria-hidden="true" />
                          <span className="capitalize text-ink/75">{integration.status}</span>
                        </div>
                      </td>
                      <td className="px-3 py-3 text-ink/70">{formatDateTime(integration.last_event_at)}</td>
                      <td className="px-3 py-3">{formatCount(integration.events_24h)}</td>
                      <td className="px-3 py-3">{formatCount(integration.errors_24h)}</td>
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
          <CardTitle>Dead Letter Queue</CardTitle>
          <AlertTriangle className="h-4 w-4 text-rust" aria-hidden="true" />
        </CardHeader>
        <CardContent>
          {dlq.isLoading ? <Skeleton className="h-48 w-full" /> : null}
          {!dlq.isLoading && !dlq.isError && (dlq.data?.length ?? 0) === 0 ? <EmptyState title="Dead letter queue is empty" message="Dead letter queue is empty." /> : null}
          {!dlq.isLoading && dlq.data && dlq.data.length > 0 ? (
            <div className="thin-scrollbar overflow-auto rounded-md border border-line">
              <table className="w-full min-w-[860px] border-collapse text-left text-sm">
                <thead className="bg-soft text-xs uppercase text-ink/55">
                  <tr>
                    <th className="px-3 py-2 font-medium">Job ID</th>
                    <th className="px-3 py-2 font-medium">Workspace ID</th>
                    <th className="px-3 py-2 font-medium">Error Message</th>
                    <th className="px-3 py-2 font-medium">Attempts</th>
                    <th className="px-3 py-2 font-medium">Created At</th>
                    <th className="px-3 py-2 font-medium">Actions</th>
                  </tr>
                </thead>
                <tbody>
                  {dlq.data.map((entry) => (
                    <tr key={entry.job_id} className="border-t border-line align-top">
                      <td className="whitespace-nowrap px-3 py-3 font-mono text-xs text-ink/70">{truncateId(entry.job_id)}</td>
                      <td className="whitespace-nowrap px-3 py-3 font-mono text-xs text-ink/70">{truncateId(entry.workspace_id)}</td>
                      <td className="max-w-md px-3 py-3 text-rust">{entry.error_message}</td>
                      <td className="px-3 py-3">{entry.attempts}</td>
                      <td className="whitespace-nowrap px-3 py-3 text-ink/70">{formatDateTime(entry.created_at)}</td>
                      <td className="px-3 py-3">
                        <div className="flex items-center gap-2">
                          <Button type="button" variant="secondary" size="sm" onClick={() => retryMutation.mutate(entry.job_id)} disabled={retryMutation.isPending || discardMutation.isPending}>
                            <RotateCcw className="h-4 w-4" aria-hidden="true" />
                            Retry
                          </Button>
                          <Button type="button" variant="destructive" size="sm" onClick={() => discardMutation.mutate(entry.job_id)} disabled={discardMutation.isPending}>
                            <Trash2 className="h-4 w-4" aria-hidden="true" />
                            Discard
                          </Button>
                        </div>
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          ) : null}
        </CardContent>
      </Card>
    </div>
  );
}

function statusDotClass(status: string): string {
  if (status === "active") {
    return "bg-green-500";
  }
  if (status === "degraded") {
    return "bg-amber-500";
  }
  if (status === "failing") {
    return "bg-red-500";
  }
  return "bg-zinc-400";
}

function truncateId(value: string): string {
  if (value.length <= 13) {
    return value;
  }
  return `${value.slice(0, 8)}...${value.slice(-4)}`;
}
