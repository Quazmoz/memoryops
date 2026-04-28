import { AlertTriangle, CheckCircle2, ChevronDown, ChevronRight, Loader2, PlugZap, RefreshCw, RotateCcw, Trash2 } from "lucide-react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Fragment, useState } from "react";

import { discardDlqJob, listDlqJobs, listIntegrations, retryDlqJob, type DlqJob } from "../api/integrations";
import { EmptyState } from "../components/EmptyState";
import { InlineError } from "../components/InlineError";
import { Badge } from "../components/ui/badge";
import { Button } from "../components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "../components/ui/card";
import { Skeleton } from "../components/ui/skeleton";
import { formatCount, formatDateTime, formatRelativeTime, previewText } from "../lib/format";
import { cn } from "../lib/utils";
import { useAppStore } from "../store/app-store";

type DlqMutationContext = { previous: DlqJob[] | undefined };
type PendingDlqJob = { job: DlqJob; action: "retry" | "discard" };

export function IntegrationsView() {
  const workspaceId = useAppStore((state) => state.workspaceId);
  const apiKey = useAppStore((state) => state.apiKey);
  const queryClient = useQueryClient();
  const [expandedJobIds, setExpandedJobIds] = useState<Set<string>>(() => new Set());
  const [rowErrors, setRowErrors] = useState<Record<string, string>>({});
  const [pendingJobs, setPendingJobs] = useState<Record<string, PendingDlqJob>>({});
  const [notice, setNotice] = useState<string | null>(null);
  const authReady = workspaceId.trim().length > 0 && apiKey.trim().length > 0;
  const integrationsQueryKey = ["workspace", workspaceId, "integrations"] as const;
  const dlqQueryKey = ["workspace", workspaceId, "dlq"] as const;
  const integrations = useQuery({
    queryKey: integrationsQueryKey,
    queryFn: () => listIntegrations(workspaceId),
    enabled: authReady,
  });
  const dlq = useQuery({
    queryKey: dlqQueryKey,
    queryFn: () => listDlqJobs(workspaceId),
    enabled: authReady,
  });
  const retryMutation = useMutation<void, Error, string, DlqMutationContext>({
    mutationFn: (jobId: string) => retryDlqJob(workspaceId, jobId),
    onMutate: (jobId) => removeDlqJob(jobId, "retry"),
    onSuccess: (_data, jobId) => {
      removePendingJob(jobId);
      setNotice(`Retry queued for ${truncateId(jobId)}.`);
    },
    onError: (error, jobId, context) => {
      restoreDlqJobs(context);
      removePendingJob(jobId);
      setRowErrors((current) => ({ ...current, [jobId]: error.message }));
    },
    onSettled: () => {
      void queryClient.invalidateQueries({ queryKey: dlqQueryKey });
    },
  });
  const discardMutation = useMutation<void, Error, string, DlqMutationContext>({
    mutationFn: (jobId) => discardDlqJob(workspaceId, jobId),
    onMutate: (jobId) => removeDlqJob(jobId, "discard"),
    onSuccess: (_data, jobId) => {
      removePendingJob(jobId);
      setNotice(`Discarded ${truncateId(jobId)}.`);
    },
    onError: (error, jobId, context) => {
      restoreDlqJobs(context);
      removePendingJob(jobId);
      setRowErrors((current) => ({ ...current, [jobId]: error.message }));
    },
    onSettled: () => {
      void queryClient.invalidateQueries({ queryKey: dlqQueryKey });
    },
  });
  const pendingRows = Object.values(pendingJobs);
  const pendingJobIds = new Set(pendingRows.map(({ job }) => job.id));
  const dlqJobs = [...pendingRows.map(({ job }) => job), ...(dlq.data ?? []).filter((job) => !pendingJobIds.has(job.id))];
  const dlqCount = dlqJobs.length;

  async function removeDlqJob(jobId: string, action: PendingDlqJob["action"]): Promise<DlqMutationContext> {
    await queryClient.cancelQueries({ queryKey: dlqQueryKey });
    const previous = queryClient.getQueryData<DlqJob[]>(dlqQueryKey);
    const job = previous?.find((entry) => entry.id === jobId);
    if (job) {
      setPendingJobs((current) => ({ ...current, [jobId]: { job, action } }));
    }
    queryClient.setQueryData<DlqJob[]>(dlqQueryKey, (current) => current?.filter((job) => job.id !== jobId) ?? []);
    setExpandedJobIds((current) => {
      const next = new Set(current);
      next.delete(jobId);
      return next;
    });
    setRowErrors((current) => {
      const next = { ...current };
      delete next[jobId];
      return next;
    });
    setNotice(null);
    return { previous };
  }

  function removePendingJob(jobId: string) {
    setPendingJobs((current) => {
      const next = { ...current };
      delete next[jobId];
      return next;
    });
  }

  function restoreDlqJobs(context: DlqMutationContext | undefined) {
    if (context?.previous !== undefined) {
      queryClient.setQueryData<DlqJob[]>(dlqQueryKey, context.previous);
    }
  }

  function toggleExpandedJob(jobId: string) {
    setExpandedJobIds((current) => {
      const next = new Set(current);
      if (next.has(jobId)) {
        next.delete(jobId);
      } else {
        next.add(jobId);
      }
      return next;
    });
  }

  return (
    <div className="mx-auto grid max-w-7xl gap-5">
      <header className="flex flex-col gap-4 sm:flex-row sm:items-center sm:justify-between">
        <div>
          <p className="text-sm font-medium text-accent-strong">Operations</p>
          <h1 className="mt-1 text-2xl font-semibold tracking-normal text-ink">Integrations</h1>
        </div>
        <Button type="button" variant="secondary" size="sm" onClick={() => void Promise.all([integrations.refetch(), dlq.refetch()])} disabled={!authReady}>
          <RefreshCw className="h-4 w-4" aria-hidden="true" />
          Refresh
        </Button>
      </header>

      {integrations.isError ? <InlineError title="Integrations unavailable" message={integrations.error.message} /> : null}
      {dlq.isError ? <InlineError title="DLQ unavailable" message={dlq.error.message} /> : null}

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

      <Card data-testid="dlq-panel">
        <CardHeader className="flex flex-row items-center justify-between space-y-0">
          <div className="flex items-center gap-2">
            <CardTitle>Dead Letter Queue</CardTitle>
            {dlqCount > 0 ? <Badge variant="rust">{formatCount(dlqCount)}</Badge> : null}
          </div>
          <AlertTriangle className="h-4 w-4 text-rust" aria-hidden="true" />
        </CardHeader>
        <CardContent className="grid gap-3">
          {notice ? (
            <div className="flex items-center gap-2 rounded-md border border-green-200 bg-green-50 px-3 py-2 text-sm text-green-700" role="status">
              <CheckCircle2 className="h-4 w-4" aria-hidden="true" />
              <span>{notice}</span>
            </div>
          ) : null}
          {dlq.isLoading ? <Skeleton className="h-48 w-full" /> : null}
          {!dlq.isLoading && !dlq.isError && dlqCount === 0 ? (
            <div className="flex items-center gap-2 rounded-md border border-green-200 bg-green-50 px-3 py-2 text-sm text-green-700">
              <CheckCircle2 className="h-4 w-4" aria-hidden="true" />
              <span>All clear — no failed jobs</span>
            </div>
          ) : null}
          {!dlq.isLoading && dlqJobs.length > 0 ? (
            <div className="thin-scrollbar overflow-auto rounded-md border border-line">
              <table className="w-full min-w-[820px] border-collapse text-left text-sm">
                <thead className="bg-soft text-xs uppercase text-ink/55">
                  <tr>
                    <th className="w-10 px-3 py-2 font-medium" aria-label="Expand" />
                    <th className="px-3 py-2 font-medium">Source</th>
                    <th className="px-3 py-2 font-medium">Failed</th>
                    <th className="px-3 py-2 font-medium">Retries</th>
                    <th className="px-3 py-2 font-medium">Error Message</th>
                    <th className="px-3 py-2 text-right font-medium">Actions</th>
                  </tr>
                </thead>
                <tbody>
                  {dlqJobs.map((job) => {
                    const expanded = expandedJobIds.has(job.id);
                    const pendingAction = pendingJobs[job.id]?.action;
                    const retrying = pendingAction === "retry";
                    const discarding = pendingAction === "discard";
                    const rowBusy = pendingAction !== undefined;
                    const rowError = rowErrors[job.id];

                    return (
                      <Fragment key={job.id}>
                        <tr className="border-t border-line align-top">
                          <td className="px-3 py-3">
                            <Button
                              type="button"
                              variant="ghost"
                              size="icon"
                              className="h-8 w-8"
                              onClick={() => toggleExpandedJob(job.id)}
                              aria-expanded={expanded}
                              aria-label={expanded ? "Collapse raw payload" : "Expand raw payload"}
                              title={expanded ? "Collapse payload" : "Expand payload"}
                            >
                              {expanded ? <ChevronDown className="h-4 w-4" aria-hidden="true" /> : <ChevronRight className="h-4 w-4" aria-hidden="true" />}
                            </Button>
                          </td>
                          <td className="px-3 py-3">
                            <Badge variant={sourceBadgeVariant(job.source)} className="capitalize">{job.source}</Badge>
                          </td>
                          <td className="whitespace-nowrap px-3 py-3 text-ink/70" title={formatDateTime(job.failed_at)}>{formatRelativeTime(job.failed_at)}</td>
                          <td className="px-3 py-3">{formatCount(job.retry_count)}</td>
                          <td className="max-w-md px-3 py-3">
                            <span className="block truncate text-rust" title={job.error_message}>{previewText(job.error_message || "No error message recorded.", 120)}</span>
                          </td>
                          <td className="px-3 py-3">
                            <div className="flex items-center justify-end gap-2">
                              <Button
                                type="button"
                                variant="secondary"
                                size="icon"
                                data-testid="dlq-retry-button"
                                onClick={() => retryMutation.mutate(job.id)}
                                disabled={rowBusy}
                                aria-label="Retry failed job"
                                title="Retry job"
                              >
                                {retrying ? <Loader2 className="h-4 w-4 animate-spin" aria-hidden="true" /> : <RotateCcw className="h-4 w-4" aria-hidden="true" />}
                              </Button>
                              <Button
                                type="button"
                                variant="ghost"
                                size="icon"
                                className="text-ink/65 hover:bg-orange-50 hover:text-rust"
                                onClick={() => discardMutation.mutate(job.id)}
                                disabled={rowBusy}
                                aria-label="Discard failed job"
                                title="Discard job"
                              >
                                {discarding ? <Loader2 className="h-4 w-4 animate-spin" aria-hidden="true" /> : <Trash2 className="h-4 w-4" aria-hidden="true" />}
                              </Button>
                            </div>
                          </td>
                        </tr>
                        {expanded ? (
                          <tr className="border-t border-line bg-soft/60">
                            <td aria-hidden="true" />
                            <td colSpan={5} className="px-3 py-3">
                              <pre className="thin-scrollbar max-h-72 overflow-auto rounded-md border border-line bg-white p-3 text-xs text-ink">{formatPayload(job.payload)}</pre>
                            </td>
                          </tr>
                        ) : null}
                        {rowError ? (
                          <tr className="border-t border-line">
                            <td aria-hidden="true" />
                            <td colSpan={5} className="px-3 py-3">
                              <InlineError title="DLQ action failed" message={rowError} />
                            </td>
                          </tr>
                        ) : null}
                      </Fragment>
                    );
                  })}
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

function sourceBadgeVariant(source: string): "blue" | "purple" | "teal" | "gray" | "muted" {
  if (source === "slack") {
    return "purple";
  }
  if (source === "linear") {
    return "blue";
  }
  if (source === "jira") {
    return "teal";
  }
  if (source === "github") {
    return "gray";
  }
  return "muted";
}

function formatPayload(payload: DlqJob["payload"]): string {
  return JSON.stringify(payload, null, 2);
}
