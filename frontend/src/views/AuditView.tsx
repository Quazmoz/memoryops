import { ChevronLeft, ChevronRight, Clipboard, Clock, ScrollText, Shield } from "lucide-react";
import { useQuery } from "@tanstack/react-query";
import { flexRender, getCoreRowModel, useReactTable, type ColumnDef } from "@tanstack/react-table";
import { useEffect, useMemo, useState, type Dispatch, type SetStateAction } from "react";

import { cn } from "../lib/utils";

import { listAuditEvents } from "../api/audit";
import type { AuditEvent } from "../api/types";
import { EmptyState } from "../components/EmptyState";
import { InlineError } from "../components/InlineError";
import { Badge } from "../components/ui/badge";
import { Button } from "../components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "../components/ui/card";
import { Skeleton } from "../components/ui/skeleton";
import { HelpTooltip, InfoLabel, Tooltip, TooltipContent, TooltipTrigger } from "../components/ui/tooltip";
import { LIVE_QUERY_INTERVALS, liveRefetchInterval } from "../hooks/use-live-query";
import { useAppStore } from "../store/app-store";

const PAGE_SIZE = 50;

export function AuditView() {
  const workspaceId = useAppStore((state) => state.workspaceId);
  const apiKey = useAppStore((state) => state.apiKey);
  const [pageIndex, setPageIndex] = useState(0);
  const [cursors, setCursors] = useState<(string | null)[]>([null]);
  const [activeTab, setActiveTab] = useState<"activity" | "compliance">("activity");
  const [cursorWorkspaceId, setCursorWorkspaceId] = useState(workspaceId);
  const authReady = workspaceId.trim().length > 0 && apiKey.trim().length > 0;
  const paginationReady = cursorWorkspaceId === workspaceId;
  const cursor = cursors[pageIndex] ?? null;

  useEffect(() => {
    setCursorWorkspaceId(workspaceId);
    setPageIndex(0);
    setCursors([null]);
  }, [workspaceId]);

  const audit = useQuery({
    queryKey: ["workspace", workspaceId, "audit", cursor],
    queryFn: () => listAuditEvents(workspaceId, { limit: PAGE_SIZE, cursor }),
    enabled: authReady && paginationReady,
    staleTime: 30_000,
    refetchInterval: pageIndex === 0 ? liveRefetchInterval(authReady && paginationReady, LIVE_QUERY_INTERVALS.audit) : false,
    refetchIntervalInBackground: false,
  });
  const items = audit.data?.items ?? [];
  const nextCursor = audit.data?.next_cursor ?? null;
  const columns = useMemo<ColumnDef<AuditEvent>[]>(
    () => [
      {
        accessorKey: "occurred_at",
        header: () => <InfoLabel label="Time" tooltip="When the audited operation happened." />,
        cell: ({ row }) => <TimeCell value={row.original.occurred_at} />,
      },
      {
        accessorKey: "actor",
        header: () => <InfoLabel label="Actor" tooltip="User, API key, system process, or automation that performed the action." />,
        cell: ({ row }) => <span className="font-mono text-xs text-ink/75">{row.original.actor}</span>,
      },
      {
        accessorKey: "action",
        header: () => <InfoLabel label="Action" tooltip="Audited operation, such as memory update, export, import, promotion, or settings change." />,
        cell: ({ row }) => <Badge variant="accent">{row.original.action}</Badge>,
      },
      {
        accessorKey: "target_type",
        header: () => <InfoLabel label="Target Type" tooltip="Category of object affected by the audited action." />,
      },
      {
        accessorKey: "target_id",
        header: () => <InfoLabel label="Target ID" tooltip="Identifier of the affected object. Copy it when debugging or correlating backend logs." />,
        cell: ({ row }) => <CopyIdButton value={row.original.target_id} />,
      },
    ],
    [],
  );
  const table = useReactTable({
    data: items,
    columns,
    getCoreRowModel: getCoreRowModel(),
  });

  return (
    <div className="mx-auto grid max-w-7xl gap-5">
      <header className="flex flex-col gap-4 sm:flex-row sm:items-center sm:justify-between">
        <div>
          <p className="text-sm font-medium text-accent-strong">Operations</p>
          <h1 className="mt-1 text-2xl font-semibold tracking-normal text-ink">Audit Log</h1>
        </div>
        {activeTab === "activity" && (
          <div className="flex items-center gap-2">
            <Badge variant="muted">{items.length} rows</Badge>
            <HelpTooltip label="Rows badge">Number of audit events currently loaded into this page of the log.</HelpTooltip>
          </div>
        )}
      </header>

      <div className="flex overflow-x-auto thin-scrollbar border-b border-line" role="tablist" aria-label="Audit sections">
        <Tooltip>
          <TooltipTrigger asChild>
            <button
              type="button"
              role="tab"
              aria-selected={activeTab === "activity"}
              onClick={() => setActiveTab("activity")}
              className={cn(
                "shrink-0 whitespace-nowrap border-b-2 px-4 pb-2.5 pt-1.5 text-sm font-medium transition-colors",
                activeTab === "activity"
                  ? "border-accent text-accent-strong"
                  : "border-transparent text-ink/55 hover:border-line hover:text-ink",
              )}
            >
              Activity Log
            </button>
          </TooltipTrigger>
          <TooltipContent>Inspect all operator activity and system events recorded in the workspace.</TooltipContent>
        </Tooltip>
        <Tooltip>
          <TooltipTrigger asChild>
            <button
              type="button"
              role="tab"
              aria-selected={activeTab === "compliance"}
              onClick={() => setActiveTab("compliance")}
              className={cn(
                "shrink-0 whitespace-nowrap border-b-2 px-4 pb-2.5 pt-1.5 text-sm font-medium transition-colors inline-flex items-center gap-1.5",
                activeTab === "compliance"
                  ? "border-accent text-accent-strong"
                  : "border-transparent text-ink/55 hover:border-line hover:text-ink",
              )}
            >
              Compliance Logs
              <span className="inline-flex items-center gap-1 rounded-full bg-amber-100/80 px-1.5 py-0.5 text-[9px] font-semibold text-amber-800 animate-pulse">
                WIP
              </span>
            </button>
          </TooltipTrigger>
          <TooltipContent>GDPR/CCPA user erasure actions and automatic data retention purge logs.</TooltipContent>
        </Tooltip>
      </div>

      {activeTab === "activity" && audit.isError ? <InlineError message={audit.error.message} /> : null}

      {activeTab === "activity" && (
        <Card>
          <CardHeader className="flex flex-row items-center justify-between space-y-0">
            <CardTitle className="flex items-center gap-1.5">
              <span>Activity</span>
              <HelpTooltip label="Activity">Audited operations performed in this workspace, including system and operator actions.</HelpTooltip>
            </CardTitle>
            <ScrollText className="h-4 w-4 text-accent-strong" aria-hidden="true" />
          </CardHeader>
          <CardContent>
            {audit.isLoading ? <Skeleton className="h-72 w-full" /> : null}
            {!audit.isLoading && !audit.isError && items.length === 0 ? (
              <EmptyState title="No audit events" message="No audit events yet — actions you take will appear here." />
            ) : null}
            {!audit.isLoading && items.length > 0 ? (
              <div className="thin-scrollbar overflow-auto rounded-md border border-line">
                <table className="w-full min-w-[780px] border-collapse text-left text-sm">
                  <thead className="bg-soft text-xs uppercase text-ink/55">
                    {table.getHeaderGroups().map((headerGroup) => (
                      <tr key={headerGroup.id}>
                        {headerGroup.headers.map((header) => (
                          <th key={header.id} className="px-3 py-2 font-medium">
                            {header.isPlaceholder ? null : flexRender(header.column.columnDef.header, header.getContext())}
                          </th>
                        ))}
                      </tr>
                    ))}
                  </thead>
                  <tbody>
                    {table.getRowModel().rows.map((row) => (
                      <tr key={row.id} className="border-t border-line align-top">
                        {row.getVisibleCells().map((cell) => (
                          <td key={cell.id} className="px-3 py-3 text-ink/70">
                            {flexRender(cell.column.columnDef.cell, cell.getContext())}
                          </td>
                        ))}
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            ) : null}

            <div className="mt-4 flex items-center justify-end gap-2">
              <Tooltip>
                <TooltipTrigger asChild>
                  <Button type="button" variant="secondary" size="sm" onClick={() => setPageIndex((index) => Math.max(0, index - 1))} disabled={pageIndex === 0 || audit.isFetching}>
                    <ChevronLeft className="h-4 w-4" aria-hidden="true" />
                    Prev
                  </Button>
                </TooltipTrigger>
                <TooltipContent>Load the previous page of audit events.</TooltipContent>
              </Tooltip>
              <Tooltip>
                <TooltipTrigger asChild>
                  <Button type="button" variant="secondary" size="sm" onClick={() => goToNextPage(nextCursor, pageIndex, setCursors, setPageIndex)} disabled={!nextCursor || audit.isFetching}>
                    Next
                    <ChevronRight className="h-4 w-4" aria-hidden="true" />
                  </Button>
                </TooltipTrigger>
                <TooltipContent>Load the next page of audit events.</TooltipContent>
              </Tooltip>
            </div>
          </CardContent>
        </Card>
      )}

      {activeTab === "compliance" && (
        <Card className="border-line/60 bg-soft/10">
          <CardHeader className="flex flex-row items-start justify-between space-y-0 pb-3">
            <div>
              <CardTitle className="flex items-center gap-1.5 text-ink/75">
                <span>Compliance Deletion Logs</span>
                <HelpTooltip label="Compliance Logs">GDPR right-to-erasure and automatic data retention logs.</HelpTooltip>
              </CardTitle>
              <p className="mt-1 text-xs text-ink/45">Work in Progress — Compliance reporting engine in development</p>
            </div>
            <div className="flex items-center gap-2">
              <span className="inline-flex items-center gap-1 rounded-full bg-amber-100/80 px-2 py-0.5 text-[10px] font-semibold text-amber-800 animate-pulse">
                <span className="h-1.5 w-1.5 rounded-full bg-amber-500" />
                WIP
              </span>
              <Shield className="h-4 w-4 text-ink/40" aria-hidden="true" />
            </div>
          </CardHeader>
          <CardContent className="grid gap-6">
            <p className="text-sm text-ink/60 leading-relaxed">
              Compliance deletion audits are securely logged in the database (`compliance_audit_log` table), including record purges and automatic data retention limits. The upcoming frontend panel will display:
            </p>
            <div className="grid gap-4 sm:grid-cols-2">
              <div className="rounded-lg border border-dashed border-line bg-white/40 p-4 flex flex-col gap-2">
                <div className="flex items-center gap-1.5">
                  <Shield className="h-4 w-4 text-ink/50" />
                  <span className="text-sm font-semibold text-ink/70">GDPR / CCPA Erasure Trail</span>
                </div>
                <p className="text-xs text-ink/45 leading-relaxed">
                  Timestamped records of user data erasure calls (`DELETE /v1/workspaces/:id/forget/user/:user_id`), noting the count of episodic memories and webhook events purged.
                </p>
                <div className="mt-auto pt-3 flex justify-between text-[11px] font-mono text-ink/30 border-t border-line/40">
                  <span>Action Status</span>
                  <span>PENDING VIEWER</span>
                </div>
              </div>
              
              <div className="rounded-lg border border-dashed border-line bg-white/40 p-4 flex flex-col gap-2">
                <div className="flex items-center gap-1.5">
                  <Clock className="h-4 w-4 text-ink/50" />
                  <span className="text-sm font-semibold text-ink/70">Retention Purge Scheduler</span>
                </div>
                <p className="text-xs text-ink/45 leading-relaxed">
                  Daily logs of the automatic background retention agent cleaning up memories older than your configured threshold (e.g. 365 days max age limit).
                </p>
                <div className="mt-auto pt-3 flex justify-between text-[11px] font-mono text-ink/30 border-t border-line/40">
                  <span>Action Status</span>
                  <span>PENDING VIEWER</span>
                </div>
              </div>
            </div>
          </CardContent>
        </Card>
      )}
    </div>
  );
}

function goToNextPage(
  nextCursor: string | null,
  pageIndex: number,
  setCursors: Dispatch<SetStateAction<(string | null)[]>>,
  setPageIndex: Dispatch<SetStateAction<number>>,
) {
  if (!nextCursor) {
    return;
  }

  setCursors((current) => [...current.slice(0, pageIndex + 1), nextCursor]);
  setPageIndex(pageIndex + 1);
}

function TimeCell({ value }: { value: string }) {
  const date = new Date(value);
  const absolute = Number.isNaN(date.getTime()) ? value : date.toLocaleString();
  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <time dateTime={value} tabIndex={0} className="whitespace-nowrap rounded-sm text-ink/70 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent">
          {formatRelativeTime(value)}
        </time>
      </TooltipTrigger>
      <TooltipContent>{absolute}</TooltipContent>
    </Tooltip>
  );
}

function CopyIdButton({ value }: { value: string }) {
  function copy() {
    void navigator.clipboard.writeText(value);
  }

  return (
    <div className="flex items-center gap-2">
      <span className="font-mono text-xs text-ink/70">{truncateUuid(value)}</span>
      <Tooltip>
        <TooltipTrigger asChild>
          <Button type="button" variant="ghost" size="icon" className="h-7 w-7" onClick={copy} aria-label="Copy target id">
            <Clipboard className="h-3.5 w-3.5" aria-hidden="true" />
          </Button>
        </TooltipTrigger>
        <TooltipContent>Copy the full target identifier for debugging or log correlation.</TooltipContent>
      </Tooltip>
    </div>
  );
}

function truncateUuid(value: string): string {
  if (value.length <= 13) {
    return value;
  }
  return `${value.slice(0, 8)}...${value.slice(-4)}`;
}

function formatRelativeTime(value: string): string {
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) {
    return value;
  }

  const diffSeconds = Math.round((date.getTime() - Date.now()) / 1000);
  const absSeconds = Math.abs(diffSeconds);
  const formatter = new Intl.RelativeTimeFormat("en", { numeric: "auto" });

  if (absSeconds < 60) {
    return formatter.format(diffSeconds, "second");
  }
  if (absSeconds < 3600) {
    return formatter.format(Math.round(diffSeconds / 60), "minute");
  }
  if (absSeconds < 86_400) {
    return formatter.format(Math.round(diffSeconds / 3600), "hour");
  }
  return formatter.format(Math.round(diffSeconds / 86_400), "day");
}
