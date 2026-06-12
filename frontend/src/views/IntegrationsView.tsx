import { AlertTriangle, Bot, CheckCircle2, ChevronDown, ChevronRight, Loader2, Plus, PlugZap, RefreshCw, RotateCcw, Trash2 } from "lucide-react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Fragment, useState, type FormEvent } from "react";

import { apiUrl } from "../api/client";
import {
  createIntegration,
  deleteIntegration,
  discardDlqJob,
  INTEGRATION_SOURCES,
  listDlqJobs,
  listIntegrations,
  retryDlqJob,
  startConnectorSync,
  type ConnectorSyncResponse,
  type DlqJob,
  type IntegrationSource,
} from "../api/integrations";
import { listMemory } from "../api/memory";
import type { IntegrationResponse, MemoryUnit } from "../api/types";
import { EmptyState } from "../components/EmptyState";
import { InlineError } from "../components/InlineError";
import { Badge } from "../components/ui/badge";
import { Button } from "../components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "../components/ui/card";
import { Input } from "../components/ui/input";
import { Skeleton } from "../components/ui/skeleton";
import { HelpTooltip, InfoLabel, Tooltip, TooltipContent, TooltipTrigger } from "../components/ui/tooltip";
import { formatCount, formatDateTime, formatRelativeTime, previewText } from "../lib/format";
import { cn } from "../lib/utils";
import { useAppStore } from "../store/app-store";

type DlqMutationContext = { previous: DlqJob[] | undefined };
type PendingDlqJob = { job: DlqJob; action: "retry" | "discard" };
type ActiveTab = "integrations" | "observations" | "dlq";
type SetupMode = "api_sync" | "webhook_only";

export function IntegrationsView() {
  const workspaceId = useAppStore((state) => state.workspaceId);
  const apiKey = useAppStore((state) => state.apiKey);
  const queryClient = useQueryClient();
  const [activeTab, setActiveTab] = useState<ActiveTab>("integrations");
  const [expandedJobIds, setExpandedJobIds] = useState<Set<string>>(() => new Set());
  const [rowErrors, setRowErrors] = useState<Record<string, string>>({});
  const [pendingJobs, setPendingJobs] = useState<Record<string, PendingDlqJob>>({});
  const [notice, setNotice] = useState<string | null>(null);
  const [setupMode, setSetupMode] = useState<SetupMode>("api_sync");
  const [addSource, setAddSource] = useState<IntegrationSource>("github");
  const [addSecret, setAddSecret] = useState("");
  const [addApiToken, setAddApiToken] = useState("");
  const [syncRepo, setSyncRepo] = useState("");
  const [syncSince, setSyncSince] = useState("");
  const [syncLimit, setSyncLimit] = useState("25");
  const [createdSource, setCreatedSource] = useState<IntegrationSource | null>(null);
  const [lastSyncResult, setLastSyncResult] = useState<ConnectorSyncResponse | null>(null);
  const [integrationToDelete, setIntegrationToDelete] = useState<string | null>(null);

  const authReady = workspaceId.trim().length > 0 && apiKey.trim().length > 0;
  const integrationsQueryKey = ["workspace", workspaceId, "integrations"] as const;
  const dlqQueryKey = ["workspace", workspaceId, "dlq"] as const;
  const observationsQueryKey = ["workspace", workspaceId, "observations"] as const;

  const integrations = useQuery({ queryKey: integrationsQueryKey, queryFn: () => listIntegrations(workspaceId), enabled: authReady });
  const dlq = useQuery({ queryKey: dlqQueryKey, queryFn: () => listDlqJobs(workspaceId), enabled: authReady });
  const observations = useQuery({
    queryKey: observationsQueryKey,
    queryFn: () => listMemory(workspaceId, { source: "observation", sort: "created_at", direction: "desc", limit: 50 }),
    enabled: authReady && activeTab === "observations",
  });

  const connectorSyncMutation = useMutation<ConnectorSyncResponse, Error, { source: IntegrationSource; repo: string; since?: string; limit?: number }>({
    mutationKey: ["workspace", workspaceId, "integrations", "sync"],
    mutationFn: ({ source, repo, since, limit }) => startConnectorSync(workspaceId, source, { repo, since, limit }),
    onSuccess: (result) => {
      setLastSyncResult(result);
      setNotice(result.message);
      void queryClient.invalidateQueries({ queryKey: integrationsQueryKey });
    },
  });

  const createIntegrationMutation = useMutation({
    mutationKey: ["workspace", workspaceId, "integrations", "create"],
    mutationFn: (request: {
      source: IntegrationSource;
      webhookSecret?: string;
      apiToken?: string;
      syncConfig: Record<string, string | number | boolean | null>;
      initialSync?: { repo: string; since?: string; limit: number };
    }) => createIntegration(workspaceId, {
      source: request.source,
      webhook_secret: request.webhookSecret,
      api_token: request.apiToken,
      api_sync_enabled: Boolean(request.apiToken),
      sync_config: request.syncConfig,
    }),
    onSuccess: (_integration, request) => {
      setAddSecret("");
      setAddApiToken("");
      setCreatedSource(request.source);
      setLastSyncResult(null);
      void queryClient.invalidateQueries({ queryKey: integrationsQueryKey });
      if (request.initialSync) {
        connectorSyncMutation.mutate({ source: request.source, ...request.initialSync });
      }
    },
  });

  const deleteIntegrationMutation = useMutation({
    mutationKey: ["workspace", workspaceId, "integrations", "delete"],
    mutationFn: (source: string) => deleteIntegration(workspaceId, source),
    onSuccess: (_data, source) => {
      setIntegrationToDelete(null);
      if (createdSource === source) setCreatedSource(null);
      void queryClient.invalidateQueries({ queryKey: integrationsQueryKey });
    },
  });

  const retryMutation = useMutation<void, Error, string, DlqMutationContext>({
    mutationFn: (jobId) => retryDlqJob(workspaceId, jobId),
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
    onSettled: () => void queryClient.invalidateQueries({ queryKey: dlqQueryKey }),
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
    onSettled: () => void queryClient.invalidateQueries({ queryKey: dlqQueryKey }),
  });

  const pendingRows = Object.values(pendingJobs);
  const pendingJobIds = new Set(pendingRows.map(({ job }) => job.id));
  const dlqJobs = [...pendingRows.map(({ job }) => job), ...(dlq.data ?? []).filter((job) => !pendingJobIds.has(job.id))];
  const dlqCount = dlqJobs.length;
  const apiMode = setupMode === "api_sync";
  const syncLimitNumber = clampSyncLimit(Number.parseInt(syncLimit, 10));

  function submitAddIntegration(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const webhookSecret = addSecret.trim();
    const apiToken = addApiToken.trim();
    const repo = syncRepo.trim();
    const since = syncSince.trim();
    const canSubmit = apiMode ? apiToken.length > 0 : webhookSecret.length > 0;
    if (!authReady || !canSubmit || createIntegrationMutation.isPending) return;

    setCreatedSource(null);
    setLastSyncResult(null);
    setNotice(null);
    createIntegrationMutation.mutate({
      source: addSource,
      ...(webhookSecret ? { webhookSecret } : {}),
      ...(apiMode && apiToken ? { apiToken } : {}),
      syncConfig: apiMode
        ? { mode: "api_sync_with_webhook", repo: repo || null, since: since || null, limit: syncLimitNumber }
        : { mode: "webhook_only" },
      ...(apiMode && addSource === "github" && repo.length > 0
        ? { initialSync: { repo, limit: syncLimitNumber, ...(since ? { since } : {}) } }
        : {}),
    });
  }

  async function removeDlqJob(jobId: string, action: PendingDlqJob["action"]): Promise<DlqMutationContext> {
    await queryClient.cancelQueries({ queryKey: dlqQueryKey });
    const previous = queryClient.getQueryData<DlqJob[]>(dlqQueryKey);
    const job = previous?.find((entry) => entry.id === jobId);
    if (job) setPendingJobs((current) => ({ ...current, [jobId]: { job, action } }));
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
    if (context?.previous !== undefined) queryClient.setQueryData<DlqJob[]>(dlqQueryKey, context.previous);
  }
  function toggleExpandedJob(jobId: string) {
    setExpandedJobIds((current) => {
      const next = new Set(current);
      next.has(jobId) ? next.delete(jobId) : next.add(jobId);
      return next;
    });
  }

  const tabs: Array<{ id: ActiveTab; label: string }> = [
    { id: "integrations", label: "Integration Health" },
    { id: "observations", label: "Observations" },
    { id: "dlq", label: `Dead Letter Queue${dlqCount > 0 ? ` (${dlqCount})` : ""}` },
  ];

  return (
    <div className="mx-auto grid max-w-7xl gap-5">
      <header className="flex flex-col gap-4 sm:flex-row sm:items-center sm:justify-between">
        <div><p className="text-sm font-medium text-accent-strong">Operations</p><h1 className="mt-1 text-2xl font-semibold tracking-normal text-ink">Integrations</h1></div>
        <Button type="button" variant="secondary" size="sm" onClick={() => void Promise.all([integrations.refetch(), dlq.refetch(), observations.refetch()])} disabled={!authReady}>
          <RefreshCw className="h-4 w-4" aria-hidden="true" />Refresh
        </Button>
      </header>

      <div className="flex overflow-x-auto thin-scrollbar border-b border-line" role="tablist" aria-label="Integrations sections">
        {tabs.map((tab) => (
          <button key={tab.id} type="button" role="tab" aria-selected={activeTab === tab.id} onClick={() => setActiveTab(tab.id)} className={cn("shrink-0 whitespace-nowrap border-b-2 px-4 pb-2.5 pt-1.5 text-sm font-medium transition-colors", activeTab === tab.id ? "border-accent text-accent-strong" : "border-transparent text-ink/55 hover:border-line hover:text-ink")}>{tab.label}</button>
        ))}
      </div>

      {activeTab === "integrations" ? (
        <>
          {integrations.isError ? <InlineError title="Integrations unavailable" message={integrations.error.message} /> : null}
          <Card data-testid="add-integration-card">
            <CardHeader className="flex flex-row items-center justify-between space-y-0"><CardTitle className="flex items-center gap-1.5"><span>Connect Integration</span><HelpTooltip label="Connect Integration">Use API sync for initial history, then webhooks for future changes.</HelpTooltip></CardTitle><Plus className="h-4 w-4 text-accent-strong" aria-hidden="true" /></CardHeader>
            <CardContent className="grid gap-4">
              <div className="grid gap-3 md:grid-cols-2">
                <SetupModeCard active={apiMode} title="Recommended: API sync + webhook" description="Store a platform credential, import existing data, and optionally configure webhooks for new changes." onClick={() => setSetupMode("api_sync")} />
                <SetupModeCard active={!apiMode} title="Advanced: webhook only" description="Only accept future provider events. Existing history is not imported." onClick={() => setSetupMode("webhook_only")} />
              </div>
              <form className="grid gap-4" onSubmit={submitAddIntegration}>
                <div className="grid gap-3 md:grid-cols-[12rem_1fr]">
                  <label className="grid gap-1 text-xs font-medium uppercase text-ink/45"><InfoLabel label="Source" tooltip="External system that feeds events into this workspace." /><select data-testid="add-integration-source" value={addSource} onChange={(event) => setAddSource(event.target.value as IntegrationSource)} className="h-10 rounded-md border border-line bg-white px-3 text-sm capitalize normal-case text-ink outline-none focus:border-accent focus:ring-2 focus:ring-accent/20">{INTEGRATION_SOURCES.map((source) => <option key={source} value={source} className="capitalize">{source}</option>)}</select></label>
                  <label className="grid gap-1 text-xs font-medium uppercase text-ink/45"><InfoLabel label={apiMode ? "Platform credential" : "Webhook secret"} tooltip={apiMode ? "Encrypted credential used for official API sync/backfill." : "Shared secret used to verify webhook signatures."} /><Input data-testid={apiMode ? "add-integration-api-token" : "add-integration-secret"} type="password" autoComplete="off" value={apiMode ? addApiToken : addSecret} onChange={(event) => apiMode ? setAddApiToken(event.target.value) : setAddSecret(event.target.value)} placeholder={apiMode ? "Platform API credential" : "Shared webhook secret"} className="normal-case" /></label>
                </div>
                {apiMode ? <div className="grid gap-3 rounded-lg border border-line bg-soft/40 p-3 md:grid-cols-[1fr_12rem_8rem]"><label className="grid gap-1 text-xs font-medium uppercase text-ink/45"><InfoLabel label="Initial GitHub repo" tooltip="owner/name repo to import recent issues and pull requests." /><Input data-testid="integration-sync-repo" value={syncRepo} onChange={(event) => setSyncRepo(event.target.value)} placeholder="Quazmoz/memoryops" disabled={addSource !== "github"} className="normal-case" /></label><label className="grid gap-1 text-xs font-medium uppercase text-ink/45"><InfoLabel label="Since" tooltip="Optional ISO timestamp." /><Input data-testid="integration-sync-since" value={syncSince} onChange={(event) => setSyncSince(event.target.value)} placeholder="2026-01-01T00:00:00Z" disabled={addSource !== "github"} className="normal-case" /></label><label className="grid gap-1 text-xs font-medium uppercase text-ink/45"><InfoLabel label="Limit" tooltip="Maximum events to queue. Capped at 100." /><Input data-testid="integration-sync-limit" value={syncLimit} onChange={(event) => setSyncLimit(event.target.value)} inputMode="numeric" disabled={addSource !== "github"} className="normal-case" /></label>{addSource !== "github" ? <p className="text-xs text-ink/60 md:col-span-3">Credential storage is enabled for this source, but the first runnable API sync adapter currently targets GitHub issues and pull requests.</p> : null}</div> : null}
                {apiMode ? <label className="grid gap-1 text-xs font-medium uppercase text-ink/45"><InfoLabel label="Webhook secret optional" tooltip="Optional secret for future provider webhooks." /><Input data-testid="add-integration-secret" type="password" autoComplete="off" value={addSecret} onChange={(event) => setAddSecret(event.target.value)} placeholder="Optional shared webhook secret" className="normal-case" /></label> : null}
                <Button type="submit" data-testid="add-integration-submit" disabled={!authReady || createIntegrationMutation.isPending || (apiMode ? addApiToken.trim().length === 0 : addSecret.trim().length === 0)}>{createIntegrationMutation.isPending ? <Loader2 className="h-4 w-4 animate-spin" aria-hidden="true" /> : <Plus className="h-4 w-4" aria-hidden="true" />}{apiMode ? "Connect integration" : "Add webhook"}</Button>
              </form>
              {createIntegrationMutation.isError ? <InlineError title="Integration could not be added" message={createIntegrationMutation.error.message} /> : null}
              {connectorSyncMutation.isError ? <InlineError title="Initial API sync could not be queued" message={connectorSyncMutation.error.message} /> : null}
              {connectorSyncMutation.isPending ? <StatusNotice message="Queueing initial API sync…" loading /> : null}
              {lastSyncResult ? <StatusNotice message={lastSyncResult.message} /> : null}
              {createdSource ? <IntegrationSetupInstructions source={createdSource} workspaceId={workspaceId} onDismiss={() => setCreatedSource(null)} /> : null}
            </CardContent>
          </Card>

          <Card><CardHeader className="flex flex-row items-center justify-between space-y-0"><CardTitle className="flex items-center gap-1.5"><span>Integration Health</span><HelpTooltip label="Integration Health">Current status and recent event volume for each source.</HelpTooltip></CardTitle><PlugZap className="h-4 w-4 text-accent-strong" aria-hidden="true" /></CardHeader><CardContent>{notice ? <StatusNotice message={notice} /> : null}{integrations.isLoading ? <Skeleton className="h-56 w-full" /> : null}{!integrations.isLoading && !integrations.isError && (integrations.data?.length ?? 0) === 0 ? <EmptyState title="No integrations configured" message="Connect a platform credential for initial API sync, then add a webhook for future changes." /> : null}{!integrations.isLoading && integrations.data && integrations.data.length > 0 ? <IntegrationTable integrations={integrations.data} onDelete={setIntegrationToDelete} onSync={(source, repo) => connectorSyncMutation.mutate({ source, repo, limit: 25 })} deleting={deleteIntegrationMutation.isPending} syncing={connectorSyncMutation.isPending} /> : null}{deleteIntegrationMutation.isError ? <InlineError title="Integration could not be removed" message={deleteIntegrationMutation.error.message} /> : null}</CardContent></Card>
          {integrationToDelete ? <RemoveIntegrationDialog source={integrationToDelete} busy={deleteIntegrationMutation.isPending} onCancel={() => setIntegrationToDelete(null)} onConfirm={() => deleteIntegrationMutation.mutate(integrationToDelete)} /> : null}
        </>
      ) : null}

      {activeTab === "observations" ? <ObservationFeed isLoading={observations.isLoading} isError={observations.isError} error={observations.error} items={observations.data?.items ?? []} /> : null}
      {activeTab === "dlq" ? <DlqPanel jobs={dlqJobs} loading={dlq.isLoading} error={dlq.isError ? dlq.error : null} notice={notice} expandedJobIds={expandedJobIds} pendingJobs={pendingJobs} rowErrors={rowErrors} onToggle={toggleExpandedJob} onRetry={(id) => retryMutation.mutate(id)} onDiscard={(id) => discardMutation.mutate(id)} /> : null}
    </div>
  );
}

function SetupModeCard({ active, title, description, onClick }: { active: boolean; title: string; description: string; onClick: () => void }) { return <button type="button" onClick={onClick} className={cn("rounded-lg border p-3 text-left transition focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent", active ? "border-accent bg-accent/10" : "border-line bg-white hover:bg-soft")}><span className="text-sm font-semibold text-ink">{title}</span><span className="mt-1 block text-xs leading-relaxed text-ink/65">{description}</span></button>; }
function StatusNotice({ message, loading = false }: { message: string; loading?: boolean }) { return <div className="flex items-center gap-2 rounded-md border border-green-200 bg-green-50 px-3 py-2 text-sm text-green-700" role="status">{loading ? <Loader2 className="h-4 w-4 animate-spin" aria-hidden="true" /> : <CheckCircle2 className="h-4 w-4" aria-hidden="true" />}<span>{message}</span></div>; }

function IntegrationTable({ integrations, onDelete, onSync, deleting, syncing }: { integrations: IntegrationResponse[]; onDelete: (source: string) => void; onSync: (source: IntegrationSource, repo: string) => void; deleting: boolean; syncing: boolean }) {
  return <div className="thin-scrollbar overflow-auto rounded-md border border-line"><table className="w-full min-w-[980px] border-collapse text-left text-sm"><thead className="bg-soft text-xs uppercase text-ink/55"><tr><th className="px-3 py-2 font-medium">Source</th><th className="px-3 py-2 font-medium">Status</th><th className="px-3 py-2 font-medium">API Sync</th><th className="px-3 py-2 font-medium">Last Sync</th><th className="px-3 py-2 font-medium">Last Event</th><th className="px-3 py-2 font-medium">Events 24h</th><th className="px-3 py-2 font-medium">Errors 24h</th><th className="px-3 py-2 text-right font-medium">Actions</th></tr></thead><tbody>{integrations.map((integration) => { const repo = repoFromSyncConfig(integration.sync_config); const canSync = integration.source === "github" && integration.has_api_credential === true && repo; return <tr key={integration.source} className="border-t border-line"><td className="px-3 py-3"><Badge variant="muted" className="capitalize">{integration.source}</Badge></td><td className="px-3 py-3"><span className={cn("mr-2 inline-block h-2.5 w-2.5 rounded-full", statusDotClass(integration.status))} /><span className="capitalize text-ink/75">{integration.status}</span></td><td className="px-3 py-3">{integration.has_api_credential ? <Badge variant={integration.api_sync_enabled ? "blue" : "muted"}>{integration.api_sync_enabled ? "Enabled" : "Credential saved"}</Badge> : <span className="text-ink/45">—</span>}</td><td className="px-3 py-3 text-ink/70">{integration.last_sync_at ? <TimestampText value={integration.last_sync_at} /> : "Pending"}</td><td className="px-3 py-3 text-ink/70">{formatDateTime(integration.last_event_at)}</td><td className="px-3 py-3">{formatCount(integration.events_24h)}</td><td className="px-3 py-3">{formatCount(integration.errors_24h)}</td><td className="px-3 py-3 text-right"><div className="flex justify-end gap-2"><Button type="button" variant="secondary" size="icon" disabled={!canSync || syncing} aria-label={`Run ${integration.source} API sync`} onClick={() => repo && onSync(integration.source as IntegrationSource, repo)}>{syncing ? <Loader2 className="h-4 w-4 animate-spin" aria-hidden="true" /> : <RefreshCw className="h-4 w-4" aria-hidden="true" />}</Button><Button type="button" variant="ghost" size="icon" className="text-ink/65 hover:bg-orange-50 hover:text-rust" data-testid={`remove-integration-${integration.source}`} aria-label={`Remove ${integration.source} integration`} disabled={deleting} onClick={() => onDelete(integration.source)}><Trash2 className="h-4 w-4" aria-hidden="true" /></Button></div></td></tr>; })}</tbody></table></div>;
}

function RemoveIntegrationDialog({ source, busy, onCancel, onConfirm }: { source: string; busy: boolean; onCancel: () => void; onConfirm: () => void }) { return <div data-testid="remove-integration-dialog" className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-xs p-4"><div className="w-full max-w-md rounded-lg border border-line bg-white p-6 shadow-xl"><h2 className="text-lg font-semibold text-ink">Remove integration</h2><p className="mt-2 text-sm text-ink/75">Remove the <strong className="capitalize">{source}</strong> integration? Webhook deliveries and API sync for this source will stop until it is added again.</p><div className="mt-6 flex justify-end gap-3"><Button type="button" variant="secondary" data-testid="cancel-remove-integration" disabled={busy} onClick={onCancel}>Cancel</Button><Button type="button" variant="destructive" data-testid="confirm-remove-integration" disabled={busy} onClick={onConfirm}>{busy ? "Removing..." : "Remove"}</Button></div></div></div>; }

function IntegrationSetupInstructions({ source, workspaceId, onDismiss }: { source: IntegrationSource; workspaceId: string; onDismiss: () => void }) {
  const webhookEndpoint = integrationWebhookEndpoint(source, workspaceId);
  return <div data-testid="integration-setup-instructions" role="status" className="grid gap-3 rounded-lg border border-green-200 bg-green-50 p-4 text-sm text-green-900"><div className="flex items-center gap-2 font-semibold"><CheckCircle2 className="h-4 w-4 shrink-0" aria-hidden="true" /><span className="capitalize">{source}</span><span className="font-normal">integration saved — API sync imports history and webhooks keep future changes fresh:</span></div>{source === "observation" ? <ol className="grid list-decimal gap-1.5 pl-5"><li>Agents submit observations directly to <code className="rounded bg-white px-1 py-0.5 font-mono text-xs">{webhookEndpoint}</code> using a workspace API key in the <code className="rounded bg-white px-1 py-0.5 font-mono text-xs">x-api-key</code> header.</li><li>MCP clients can use the built-in <code className="rounded bg-white px-1 py-0.5 font-mono text-xs">memory_store</code> tool instead.</li></ol> : <ol className="grid list-decimal gap-1.5 pl-5"><li>If you supplied a GitHub repo, MemoryOps queued the initial API sync into the same raw-event pipeline used by webhooks.</li><li>To keep future changes fresh, point the provider webhook at <code className="break-all rounded bg-white px-1 py-0.5 font-mono text-xs">{webhookEndpoint}</code></li><li>Set the provider webhook signing secret to the exact secret you entered.</li></ol>}<p className="text-xs text-green-800/80">Saved credentials and webhook secrets are encrypted and not displayed again.</p><div><Button type="button" variant="secondary" size="sm" onClick={onDismiss}>Dismiss</Button></div></div>;
}

function integrationWebhookEndpoint(source: IntegrationSource, workspaceId: string): string { const path = source === "observation" ? "/v1/ingest/observation" : `/v1/ingest/${encodeURIComponent(source)}/${encodeURIComponent(workspaceId)}`; const resolved = apiUrl(path); if (/^https?:\/\//i.test(resolved)) return resolved; if (typeof window !== "undefined" && window.location?.origin) return `${window.location.origin}${resolved}`; return resolved; }

function ObservationFeed({ isLoading, isError, error, items }: { isLoading: boolean; isError: boolean; error: Error | null; items: MemoryUnit[] }) { return <Card><CardHeader className="flex flex-row items-center justify-between space-y-0"><CardTitle className="flex items-center gap-1.5"><span>Agent Observations</span><HelpTooltip label="Agent Observations">First-party agent-submitted memories.</HelpTooltip></CardTitle><Bot className="h-4 w-4 text-accent-strong" aria-hidden="true" /></CardHeader><CardContent>{isError && error ? <InlineError title="Observations unavailable" message={error.message} /> : null}{isLoading ? <Skeleton className="h-56 w-full" /> : null}{!isLoading && !isError && items.length === 0 ? <EmptyState title="No agent observations yet" message="Use POST /v1/ingest/observation or the MCP memory_store tool to submit agent observations." /> : null}{!isLoading && items.length > 0 ? <div className="thin-scrollbar overflow-auto rounded-md border border-line"><table className="w-full min-w-[640px] border-collapse text-left text-sm"><thead className="bg-soft text-xs uppercase text-ink/55"><tr><th className="px-3 py-2 font-medium">Agent</th><th className="px-3 py-2 font-medium">Content</th><th className="px-3 py-2 font-medium">Importance</th><th className="px-3 py-2 font-medium">Created</th></tr></thead><tbody>{items.map((item) => { const agentId = scopeField(item.scope, "agent_id"); return <tr key={item.id} className="border-t border-line"><td className="whitespace-nowrap px-3 py-3"><Badge variant="blue" className="max-w-[10rem] truncate font-mono text-xs">{agentId ?? "—"}</Badge></td><td className="max-w-sm px-3 py-3 text-ink/80"><TooltipText value={item.content}>{previewText(item.content, 120)}</TooltipText></td><td className="px-3 py-3"><span className={cn("text-xs font-medium tabular-nums", importanceColor(item.importance_score))}>{item.importance_score.toFixed(2)}</span></td><td className="whitespace-nowrap px-3 py-3 text-ink/65"><TimestampText value={item.created_at} /></td></tr>; })}</tbody></table></div> : null}</CardContent></Card>; }

function DlqPanel({ jobs, loading, error, notice, expandedJobIds, pendingJobs, rowErrors, onToggle, onRetry, onDiscard }: { jobs: DlqJob[]; loading: boolean; error: Error | null; notice: string | null; expandedJobIds: Set<string>; pendingJobs: Record<string, PendingDlqJob>; rowErrors: Record<string, string>; onToggle: (id: string) => void; onRetry: (id: string) => void; onDiscard: (id: string) => void }) { return <><Card data-testid="dlq-panel"><CardHeader className="flex flex-row items-center justify-between space-y-0"><div className="flex items-center gap-2"><CardTitle className="flex items-center gap-1.5"><span>Dead Letter Queue</span><HelpTooltip label="Dead Letter Queue">Failed integration jobs.</HelpTooltip></CardTitle>{jobs.length > 0 ? <Badge variant="rust">{formatCount(jobs.length)}</Badge> : null}</div><AlertTriangle className="h-4 w-4 text-rust" aria-hidden="true" /></CardHeader><CardContent className="grid gap-3">{error ? <InlineError title="DLQ unavailable" message={error.message} /> : null}{notice ? <StatusNotice message={notice} /> : null}{loading ? <Skeleton className="h-48 w-full" /> : null}{!loading && !error && jobs.length === 0 ? <StatusNotice message="All clear — no failed jobs" /> : null}{!loading && jobs.length > 0 ? <div className="thin-scrollbar overflow-auto rounded-md border border-line"><table className="w-full min-w-[820px] border-collapse text-left text-sm"><thead className="bg-soft text-xs uppercase text-ink/55"><tr><th className="w-10 px-3 py-2" /><th className="px-3 py-2 font-medium">Source</th><th className="px-3 py-2 font-medium">Failed</th><th className="px-3 py-2 font-medium">Retries</th><th className="px-3 py-2 font-medium">Error Message</th><th className="px-3 py-2 text-right font-medium">Actions</th></tr></thead><tbody>{jobs.map((job) => { const expanded = expandedJobIds.has(job.id); const pendingAction = pendingJobs[job.id]?.action; const rowBusy = pendingAction !== undefined; const rowError = rowErrors[job.id]; return <Fragment key={job.id}><tr className="border-t border-line align-top"><td className="px-3 py-3"><Button type="button" variant="ghost" size="icon" className="h-8 w-8" onClick={() => onToggle(job.id)} aria-expanded={expanded} aria-label={expanded ? "Collapse raw payload" : "Expand raw payload"}>{expanded ? <ChevronDown className="h-4 w-4" aria-hidden="true" /> : <ChevronRight className="h-4 w-4" aria-hidden="true" />}</Button></td><td className="px-3 py-3"><Badge variant={sourceBadgeVariant(job.source)} className="capitalize">{job.source}</Badge></td><td className="whitespace-nowrap px-3 py-3 text-ink/70"><TimestampText value={job.failed_at} /></td><td className="px-3 py-3">{formatCount(job.retry_count)}</td><td className="max-w-md px-3 py-3"><TooltipText className="block truncate text-rust" value={job.error_message || "No error message recorded."}>{previewText(job.error_message || "No error message recorded.", 120)}</TooltipText></td><td className="px-3 py-3"><div className="flex items-center justify-end gap-2"><Button type="button" variant="secondary" size="icon" data-testid="dlq-retry-button" onClick={() => onRetry(job.id)} disabled={rowBusy} aria-label="Retry failed job">{pendingAction === "retry" ? <Loader2 className="h-4 w-4 animate-spin" aria-hidden="true" /> : <RotateCcw className="h-4 w-4" aria-hidden="true" />}</Button><Button type="button" variant="ghost" size="icon" className="text-ink/65 hover:bg-orange-50 hover:text-rust" onClick={() => onDiscard(job.id)} disabled={rowBusy} aria-label="Discard failed job">{pendingAction === "discard" ? <Loader2 className="h-4 w-4 animate-spin" aria-hidden="true" /> : <Trash2 className="h-4 w-4" aria-hidden="true" />}</Button></div></td></tr>{expanded ? <tr className="border-t border-line bg-soft/60"><td aria-hidden="true" /><td colSpan={5} className="px-3 py-3"><pre className="thin-scrollbar max-h-72 overflow-auto rounded-md border border-line bg-white p-3 text-xs text-ink">{JSON.stringify(job.payload, null, 2)}</pre></td></tr> : null}{rowError ? <tr className="border-t border-line"><td aria-hidden="true" /><td colSpan={5} className="px-3 py-3"><InlineError title="DLQ action failed" message={rowError} /></td></tr> : null}</Fragment>; })}</tbody></table></div> : null}</CardContent></Card></>; }

function scopeField(scope: unknown, field: string): string | null { if (!scope || typeof scope !== "object" || Array.isArray(scope)) return null; const value = (scope as Record<string, unknown>)[field]; return typeof value === "string" && value.trim().length > 0 ? value : null; }
function repoFromSyncConfig(config: unknown): string | null { if (!config || typeof config !== "object" || Array.isArray(config)) return null; const value = (config as Record<string, unknown>).repo; return typeof value === "string" && value.trim().length > 0 ? value : null; }
function clampSyncLimit(value: number): number { return Number.isFinite(value) ? Math.max(1, Math.min(100, Math.trunc(value))) : 25; }
function importanceColor(score: number): string { if (score >= 0.75) return "text-green-700"; if (score >= 0.5) return "text-amber-600"; return "text-ink/55"; }
function statusDotClass(status: string): string { if (status === "active") return "bg-green-500"; if (status === "degraded") return "bg-amber-500"; if (status === "failing") return "bg-red-500"; return "bg-zinc-400"; }
function truncateId(value: string): string { return value.length <= 13 ? value : `${value.slice(0, 8)}...${value.slice(-4)}`; }
function sourceBadgeVariant(source: string): "blue" | "purple" | "teal" | "gray" | "muted" { if (source === "slack") return "purple"; if (source === "linear") return "blue"; if (source === "jira") return "teal"; if (source === "github") return "gray"; return "muted"; }
function TooltipText({ value, children, className }: { value: string; children: React.ReactNode; className?: string }) { return <Tooltip><TooltipTrigger asChild><span tabIndex={0} className={cn("rounded-sm focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent", className)}>{children}</span></TooltipTrigger><TooltipContent>{value}</TooltipContent></Tooltip>; }
function TimestampText({ value }: { value: string }) { return <TooltipText value={formatDateTime(value)}>{formatRelativeTime(value)}</TooltipText>; }
