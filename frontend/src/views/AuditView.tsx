import { ChevronLeft, ChevronRight, Clipboard, ScrollText } from "lucide-react";
import { useQuery } from "@tanstack/react-query";
import { flexRender, getCoreRowModel, useReactTable, type ColumnDef } from "@tanstack/react-table";
import { useMemo, useState } from "react";

import { listAuditEvents } from "../api/audit";
import type { AuditEvent } from "../api/types";
import { EmptyState } from "../components/EmptyState";
import { InlineError } from "../components/InlineError";
import { Badge } from "../components/ui/badge";
import { Button } from "../components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "../components/ui/card";
import { Skeleton } from "../components/ui/skeleton";
import { useAppStore } from "../store/app-store";

const PAGE_SIZE = 50;

export function AuditView() {
  const workspaceId = useAppStore((state) => state.workspaceId);
  const apiKey = useAppStore((state) => state.apiKey);
  const [offset, setOffset] = useState(0);
  const authReady = workspaceId.trim().length > 0 && apiKey.trim().length > 0;
  const audit = useQuery({
    queryKey: ["workspace", workspaceId, "audit", offset],
    queryFn: () => listAuditEvents(workspaceId, PAGE_SIZE, offset),
    enabled: authReady,
    staleTime: 30_000,
    refetchInterval: 30_000,
  });
  const items = audit.data ?? [];
  const columns = useMemo<ColumnDef<AuditEvent>[]>(
    () => [
      {
        accessorKey: "occurred_at",
        header: "Time",
        cell: ({ row }) => <TimeCell value={row.original.occurred_at} />,
      },
      {
        accessorKey: "actor",
        header: "Actor",
        cell: ({ row }) => <span className="font-mono text-xs text-ink/75">{row.original.actor}</span>,
      },
      {
        accessorKey: "action",
        header: "Action",
        cell: ({ row }) => <Badge variant="accent">{row.original.action}</Badge>,
      },
      {
        accessorKey: "target_type",
        header: "Target Type",
      },
      {
        accessorKey: "target_id",
        header: "Target ID",
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
        <Badge variant="muted">{items.length} rows</Badge>
      </header>

      {audit.isError ? <InlineError message={audit.error.message} /> : null}

      <Card>
        <CardHeader className="flex flex-row items-center justify-between space-y-0">
          <CardTitle>Activity</CardTitle>
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
            <Button type="button" variant="secondary" size="sm" onClick={() => setOffset(Math.max(0, offset - PAGE_SIZE))} disabled={offset === 0 || audit.isFetching}>
              <ChevronLeft className="h-4 w-4" aria-hidden="true" />
              Prev
            </Button>
            <Button type="button" variant="secondary" size="sm" onClick={() => setOffset(offset + PAGE_SIZE)} disabled={items.length < PAGE_SIZE || audit.isFetching}>
              Next
              <ChevronRight className="h-4 w-4" aria-hidden="true" />
            </Button>
          </div>
        </CardContent>
      </Card>
    </div>
  );
}

function TimeCell({ value }: { value: string }) {
  const date = new Date(value);
  const absolute = Number.isNaN(date.getTime()) ? value : date.toLocaleString();
  return (
    <time dateTime={value} title={absolute} className="whitespace-nowrap text-ink/70">
      {formatRelativeTime(value)}
    </time>
  );
}

function CopyIdButton({ value }: { value: string }) {
  function copy() {
    void navigator.clipboard.writeText(value);
  }

  return (
    <div className="flex items-center gap-2">
      <span className="font-mono text-xs text-ink/70">{truncateUuid(value)}</span>
      <Button type="button" variant="ghost" size="icon" className="h-7 w-7" onClick={copy} aria-label="Copy target id">
        <Clipboard className="h-3.5 w-3.5" aria-hidden="true" />
      </Button>
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
