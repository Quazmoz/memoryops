import { ChevronDown, ChevronRight, Clipboard, Download, ScrollText, Search, ShieldCheck } from "lucide-react";
import { useInfiniteQuery, useMutation, useQuery } from "@tanstack/react-query";
import { useEffect, useMemo, useState, type ReactNode } from "react";

import {
  downloadAuditExport,
  listAuditActions,
  listAuditEvents,
  verifyAuditChain,
  type AuditFilters,
} from "../api/audit";
import type { AuditEvent, AuditChainVerification, JsonValue } from "../api/types";
import { EmptyState } from "../components/EmptyState";
import { InlineError } from "../components/InlineError";
import { Badge } from "../components/ui/badge";
import { Button } from "../components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "../components/ui/card";
import { Input } from "../components/ui/input";
import { Skeleton } from "../components/ui/skeleton";
import { HelpTooltip, Tooltip, TooltipContent, TooltipTrigger } from "../components/ui/tooltip";
import { formatDateTime, formatRelativeTime } from "../lib/format";
import { useAppStore } from "../store/app-store";

const PAGE_SIZE = 50;

type DraftFilters = {
  q: string;
  action: string;
  category: string;
  severity: string;
  success: string;
  actor: string;
  targetType: string;
  from: string;
  to: string;
};

const EMPTY_DRAFT: DraftFilters = {
  q: "",
  action: "",
  category: "",
  severity: "",
  success: "",
  actor: "",
  targetType: "",
  from: "",
  to: "",
};

function buildFilters(draft: DraftFilters): AuditFilters {
  const filters: AuditFilters = {};
  if (draft.q.trim()) filters.q = draft.q.trim();
  if (draft.action) filters.actions = draft.action;
  if (draft.category) filters.category = draft.category;
  if (draft.severity) filters.severity = draft.severity;
  if (draft.success === "true" || draft.success === "false") filters.success = draft.success === "true";
  if (draft.actor.trim()) filters.actor = draft.actor.trim();
  if (draft.targetType.trim()) filters.target_type = draft.targetType.trim();
  if (draft.from) filters.from = localToIso(draft.from);
  if (draft.to) filters.to = localToIso(draft.to);
  return filters;
}

function localToIso(local: string): string {
  const date = new Date(local);
  return Number.isNaN(date.getTime()) ? local : date.toISOString();
}

export function AuditView() {
  const workspaceId = useAppStore((state) => state.workspaceId);
  const apiKey = useAppStore((state) => state.apiKey);
  const authReady = workspaceId.trim().length > 0 && apiKey.trim().length > 0;

  const [draft, setDraft] = useState<DraftFilters>(EMPTY_DRAFT);
  const [applied, setApplied] = useState<AuditFilters>({});
  const [expanded, setExpanded] = useState<Set<string>>(new Set());
  const [verification, setVerification] = useState<AuditChainVerification | null>(null);
  const [exportError, setExportError] = useState<string | null>(null);

  useEffect(() => {
    setDraft(EMPTY_DRAFT);
    setApplied({});
    setExpanded(new Set());
    setVerification(null);
  }, [workspaceId]);

  const actionsCatalog = useQuery({
    queryKey: ["workspace", workspaceId, "audit-actions"],
    queryFn: () => listAuditActions(workspaceId),
    enabled: authReady,
    staleTime: 5 * 60_000,
  });

  const audit = useInfiniteQuery({
    queryKey: ["workspace", workspaceId, "audit", applied],
    queryFn: ({ pageParam }) =>
      listAuditEvents(workspaceId, { limit: PAGE_SIZE, cursor: pageParam, ...applied }),
    initialPageParam: null as string | null,
    getNextPageParam: (last) => last.next_cursor ?? undefined,
    enabled: authReady,
    staleTime: 30_000,
  });

  const items = useMemo(() => audit.data?.pages.flatMap((page) => page.items) ?? [], [audit.data]);

  const verifyMutation = useMutation({
    mutationFn: () => verifyAuditChain(workspaceId),
    onSuccess: setVerification,
  });

  async function runExport(format: "jsonl" | "csv") {
    setExportError(null);
    try {
      await downloadAuditExport(workspaceId, format, applied);
    } catch (error) {
      setExportError(error instanceof Error ? error.message : "Export failed");
    }
  }

  function toggleExpanded(id: string) {
    setExpanded((current) => {
      const next = new Set(current);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  }

  const severityOptions = actionsCatalog.data?.severities ?? ["info", "notice", "warning", "critical"];
  const categoryOptions = actionsCatalog.data?.categories ?? [];
  const actionOptions = actionsCatalog.data?.actions ?? [];

  return (
    <div className="mx-auto grid max-w-7xl gap-5">
      <header className="flex flex-col gap-4 sm:flex-row sm:items-center sm:justify-between">
        <div>
          <p className="text-sm font-medium text-accent-strong">Operations</p>
          <h1 className="mt-1 text-2xl font-semibold tracking-normal text-ink">Audit Log</h1>
        </div>
        <div className="flex flex-wrap items-center gap-2">
          <Badge variant="muted">{items.length} loaded</Badge>
          <Button type="button" variant="secondary" size="sm" onClick={() => void runExport("jsonl")} disabled={!authReady}>
            <Download className="h-4 w-4" aria-hidden="true" /> JSONL
          </Button>
          <Button type="button" variant="secondary" size="sm" onClick={() => void runExport("csv")} disabled={!authReady}>
            <Download className="h-4 w-4" aria-hidden="true" /> CSV
          </Button>
          <Tooltip>
            <TooltipTrigger asChild>
              <Button type="button" variant="secondary" size="sm" onClick={() => verifyMutation.mutate()} disabled={!authReady || verifyMutation.isPending}>
                <ShieldCheck className="h-4 w-4" aria-hidden="true" /> Verify
              </Button>
            </TooltipTrigger>
            <TooltipContent>Recompute and verify the tamper-evident hash chain.</TooltipContent>
          </Tooltip>
        </div>
      </header>

      {verification ? <VerificationBanner result={verification} /> : null}
      {exportError ? <InlineError message={exportError} /> : null}
      {audit.isError ? <InlineError message={audit.error.message} /> : null}

      <Card>
        <CardContent className="py-4">
          <form
            className="grid gap-3 md:grid-cols-2 lg:grid-cols-4"
            onSubmit={(event) => {
              event.preventDefault();
              setApplied(buildFilters(draft));
              setExpanded(new Set());
            }}
          >
            <label className="flex flex-col gap-1 text-xs font-medium text-ink/70 lg:col-span-2">
              Search
              <div className="relative">
                <Search className="pointer-events-none absolute left-2.5 top-2.5 h-4 w-4 text-ink/40" aria-hidden="true" />
                <Input
                  className="pl-8"
                  placeholder="actor, target, request id, reason…"
                  value={draft.q}
                  onChange={(event) => setDraft({ ...draft, q: event.target.value })}
                />
              </div>
            </label>
            <label className="flex flex-col gap-1 text-xs font-medium text-ink/70">
              Action
              <SelectNative value={draft.action} onChange={(value) => setDraft({ ...draft, action: value })}>
                <option value="">All actions</option>
                {actionOptions.map((action) => (
                  <option key={action.name} value={action.name}>
                    {action.name}
                  </option>
                ))}
              </SelectNative>
            </label>
            <label className="flex flex-col gap-1 text-xs font-medium text-ink/70">
              Category
              <SelectNative value={draft.category} onChange={(value) => setDraft({ ...draft, category: value })}>
                <option value="">All categories</option>
                {categoryOptions.map((category) => (
                  <option key={category} value={category}>
                    {category}
                  </option>
                ))}
              </SelectNative>
            </label>
            <label className="flex flex-col gap-1 text-xs font-medium text-ink/70">
              Severity
              <SelectNative value={draft.severity} onChange={(value) => setDraft({ ...draft, severity: value })}>
                <option value="">All severities</option>
                {severityOptions.map((severity) => (
                  <option key={severity} value={severity}>
                    {severity}
                  </option>
                ))}
              </SelectNative>
            </label>
            <label className="flex flex-col gap-1 text-xs font-medium text-ink/70">
              Outcome
              <SelectNative value={draft.success} onChange={(value) => setDraft({ ...draft, success: value })}>
                <option value="">Any outcome</option>
                <option value="true">Success</option>
                <option value="false">Failure</option>
              </SelectNative>
            </label>
            <label className="flex flex-col gap-1 text-xs font-medium text-ink/70">
              Actor
              <Input placeholder="api_key:…" value={draft.actor} onChange={(event) => setDraft({ ...draft, actor: event.target.value })} />
            </label>
            <label className="flex flex-col gap-1 text-xs font-medium text-ink/70">
              Target type
              <Input placeholder="workspace_tool…" value={draft.targetType} onChange={(event) => setDraft({ ...draft, targetType: event.target.value })} />
            </label>
            <label className="flex flex-col gap-1 text-xs font-medium text-ink/70">
              From
              <Input type="datetime-local" value={draft.from} onChange={(event) => setDraft({ ...draft, from: event.target.value })} />
            </label>
            <label className="flex flex-col gap-1 text-xs font-medium text-ink/70">
              To
              <Input type="datetime-local" value={draft.to} onChange={(event) => setDraft({ ...draft, to: event.target.value })} />
            </label>
            <div className="flex items-end gap-2">
              <Button type="submit" size="sm">
                Apply
              </Button>
              <Button
                type="button"
                variant="secondary"
                size="sm"
                onClick={() => {
                  setDraft(EMPTY_DRAFT);
                  setApplied({});
                  setExpanded(new Set());
                }}
              >
                Clear
              </Button>
            </div>
          </form>
        </CardContent>
      </Card>

      <Card>
        <CardHeader className="flex flex-row items-center justify-between space-y-0">
          <CardTitle className="flex items-center gap-1.5">
            <span>Activity</span>
            <HelpTooltip label="Activity">
              Security-sensitive operations are recorded reliably; high-volume operational events are best-effort. Secret values are redacted.
            </HelpTooltip>
          </CardTitle>
          <ScrollText className="h-4 w-4 text-accent-strong" aria-hidden="true" />
        </CardHeader>
        <CardContent>
          {audit.isLoading ? <Skeleton className="h-72 w-full" /> : null}
          {!audit.isLoading && !audit.isError && items.length === 0 ? (
            <EmptyState title="No audit events" message="No audit events match the current filters." />
          ) : null}
          {!audit.isLoading && items.length > 0 ? (
            <div className="thin-scrollbar overflow-auto rounded-md border border-line">
              <table className="w-full min-w-[920px] border-collapse text-left text-sm">
                <thead className="bg-soft text-xs uppercase text-ink/55">
                  <tr>
                    <th className="w-8 px-2 py-2" />
                    <th className="px-3 py-2 font-medium">Time</th>
                    <th className="px-3 py-2 font-medium">Severity</th>
                    <th className="px-3 py-2 font-medium">Action</th>
                    <th className="px-3 py-2 font-medium">Actor</th>
                    <th className="px-3 py-2 font-medium">Target</th>
                    <th className="px-3 py-2 font-medium">Source IP</th>
                    <th className="px-3 py-2 font-medium">Request</th>
                  </tr>
                </thead>
                <tbody>
                  {items.map((event) => (
                    <AuditRow
                      key={event.id}
                      event={event}
                      expanded={expanded.has(event.id)}
                      onToggle={() => toggleExpanded(event.id)}
                    />
                  ))}
                </tbody>
              </table>
            </div>
          ) : null}

          <div className="mt-4 flex items-center justify-center">
            {audit.hasNextPage ? (
              <Button
                type="button"
                variant="secondary"
                size="sm"
                onClick={() => void audit.fetchNextPage()}
                disabled={audit.isFetchingNextPage}
              >
                {audit.isFetchingNextPage ? "Loading…" : "Load more"}
              </Button>
            ) : items.length > 0 ? (
              <span className="text-xs text-ink/50">End of results</span>
            ) : null}
          </div>
        </CardContent>
      </Card>
    </div>
  );
}

function AuditRow({ event, expanded, onToggle }: { event: AuditEvent; expanded: boolean; onToggle: () => void }) {
  return (
    <>
      <tr className="border-t border-line align-top hover:bg-soft/50">
        <td className="px-2 py-3">
          <button type="button" onClick={onToggle} aria-label={expanded ? "Collapse" : "Expand"} className="text-ink/50 hover:text-ink">
            {expanded ? <ChevronDown className="h-4 w-4" /> : <ChevronRight className="h-4 w-4" />}
          </button>
        </td>
        <td className="px-3 py-3">
          <Tooltip>
            <TooltipTrigger asChild>
              <time dateTime={event.occurred_at} className="whitespace-nowrap text-ink/70">
                {formatRelativeTime(event.occurred_at)}
              </time>
            </TooltipTrigger>
            <TooltipContent>{formatDateTime(event.occurred_at)}</TooltipContent>
          </Tooltip>
        </td>
        <td className="px-3 py-3">
          <SeverityBadge severity={event.severity} success={event.success} />
        </td>
        <td className="px-3 py-3">
          <Badge variant="accent">{event.action}</Badge>
        </td>
        <td className="px-3 py-3">
          <span className="font-mono text-xs text-ink/75">{event.actor_display ?? event.actor}</span>
        </td>
        <td className="px-3 py-3">
          <div className="flex flex-col">
            <span className="text-xs text-ink/70">{event.target_type}</span>
            <span className="font-mono text-[11px] text-ink/50">{event.target_name ?? truncateUuid(event.target_id)}</span>
          </div>
        </td>
        <td className="px-3 py-3">
          <span className="font-mono text-xs text-ink/60">{event.source_ip ?? "—"}</span>
        </td>
        <td className="px-3 py-3">
          {event.request_id ? <CopyButton value={event.request_id} label={truncateUuid(event.request_id)} /> : <span className="text-ink/40">—</span>}
        </td>
      </tr>
      {expanded ? (
        <tr className="border-t border-line bg-soft/40">
          <td />
          <td colSpan={7} className="px-3 py-3">
            <AuditDetail event={event} />
          </td>
        </tr>
      ) : null}
    </>
  );
}

function AuditDetail({ event }: { event: AuditEvent }) {
  return (
    <div className="grid gap-3 text-xs">
      <div className="flex flex-wrap gap-x-6 gap-y-2">
        <Field label="Audit ID" value={event.id} copyable />
        {event.api_key_id ? <Field label="API key" value={`${event.api_key_prefix ?? ""} (${event.api_key_id})`} /> : null}
        {event.correlation_id ? <Field label="Correlation" value={event.correlation_id} copyable /> : null}
        {event.method || event.route ? <Field label="Request" value={`${event.method ?? ""} ${event.route ?? ""}`.trim()} /> : null}
        {event.user_agent ? <Field label="User agent" value={event.user_agent} /> : null}
        {event.reason ? <Field label="Reason" value={event.reason} /> : null}
        {event.error_code ? <Field label="Error" value={event.error_code} /> : null}
        {typeof event.seq === "number" ? <Field label="Chain seq" value={String(event.seq)} /> : null}
      </div>
      <JsonBlock label="Metadata" value={event.metadata} />
      <JsonBlock label="Before" value={event.before} />
      <JsonBlock label="After" value={event.after} />
      <JsonBlock label="Diff" value={event.diff} />
      <p className="text-[11px] text-ink/45">Payloads are redacted; fields shown as “[REDACTED]” had secret values removed before storage.</p>
    </div>
  );
}

function Field({ label, value, copyable }: { label: string; value: string; copyable?: boolean }) {
  return (
    <div className="flex flex-col">
      <span className="text-[10px] uppercase tracking-wide text-ink/45">{label}</span>
      {copyable ? <CopyButton value={value} label={value} /> : <span className="font-mono text-xs text-ink/70">{value}</span>}
    </div>
  );
}

function JsonBlock({ label, value }: { label: string; value: JsonValue | null | undefined }) {
  if (value === null || value === undefined) {
    return null;
  }
  const text = JSON.stringify(value, null, 2);
  const truncated = text.length > 4000;
  return (
    <details className="rounded-md border border-line bg-white">
      <summary className="cursor-pointer px-3 py-2 text-[11px] font-medium uppercase tracking-wide text-ink/55">{label}</summary>
      <pre className="thin-scrollbar max-h-72 overflow-auto px-3 pb-3 text-[11px] leading-relaxed text-ink/70">
        {truncated ? `${text.slice(0, 4000)}\n… (${text.length - 4000} more chars)` : text}
      </pre>
    </details>
  );
}

function SeverityBadge({ severity, success }: { severity: string; success: boolean }) {
  if (!success) {
    return <Badge variant="rust">failed</Badge>;
  }
  const variant =
    severity === "critical" ? "rust" : severity === "warning" ? "amber" : severity === "notice" ? "blue" : "muted";
  return <Badge variant={variant}>{severity}</Badge>;
}

function VerificationBanner({ result }: { result: AuditChainVerification }) {
  if (!result.enabled) {
    return (
      <div className="rounded-md border border-amber-200 bg-amber-50 px-4 py-3 text-sm text-amber-800">
        Tamper-evident hash chain is disabled (no signing key configured).
      </div>
    );
  }
  const tone = result.verified ? "border-green-200 bg-green-50 text-green-700" : "border-orange-200 bg-orange-50 text-orange-800";
  return (
    <div className={`rounded-md border px-4 py-3 text-sm ${tone}`}>
      {result.verified ? "✓ " : "✗ "}
      {result.message}
      {result.first_broken_seq != null ? ` (first broken seq: ${result.first_broken_seq})` : ""}
    </div>
  );
}

function CopyButton({ value, label }: { value: string; label: string }) {
  return (
    <span className="flex items-center gap-1.5">
      <span className="font-mono text-xs text-ink/70">{label}</span>
      <Tooltip>
        <TooltipTrigger asChild>
          <Button type="button" variant="ghost" size="icon" className="h-6 w-6" onClick={() => void navigator.clipboard.writeText(value)} aria-label="Copy">
            <Clipboard className="h-3 w-3" aria-hidden="true" />
          </Button>
        </TooltipTrigger>
        <TooltipContent>Copy {value}</TooltipContent>
      </Tooltip>
    </span>
  );
}

function SelectNative({
  value,
  onChange,
  children,
}: {
  value: string;
  onChange: (value: string) => void;
  children: ReactNode;
}) {
  return (
    <select
      value={value}
      onChange={(event) => onChange(event.target.value)}
      className="h-10 w-full rounded-md border border-line bg-white px-2 text-sm outline-none transition focus:border-accent focus:ring-2 focus:ring-accent/20"
    >
      {children}
    </select>
  );
}

function truncateUuid(value: string): string {
  if (value.length <= 13) {
    return value;
  }
  return `${value.slice(0, 8)}…${value.slice(-4)}`;
}
