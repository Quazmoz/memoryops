import { ChevronLeft, ChevronRight, ScrollText } from "lucide-react";
import { useQuery } from "@tanstack/react-query";
import { useState } from "react";

import { listAudit } from "../api/workspace";
import { EmptyState } from "../components/EmptyState";
import { InlineError } from "../components/InlineError";
import { Badge } from "../components/ui/badge";
import { Button } from "../components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "../components/ui/card";
import { Skeleton } from "../components/ui/skeleton";
import { useAppStore } from "../store/app-store";

const PAGE_SIZE = 25;

export function AuditView() {
  const workspaceId = useAppStore((state) => state.workspaceId);
  const [offset, setOffset] = useState(0);
  const audit = useQuery({
    queryKey: ["workspace", workspaceId, "audit", offset],
    queryFn: () => listAudit(workspaceId, { limit: PAGE_SIZE, offset }),
  });
  const items = audit.data?.items ?? [];

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
            <EmptyState title="No audit entries" message="Workspace activity will appear here after API actions run." />
          ) : null}
          {!audit.isLoading && items.length > 0 ? (
            <div className="thin-scrollbar overflow-auto rounded-md border border-line">
              <table className="w-full min-w-[760px] border-collapse text-left text-sm">
                <thead className="bg-soft text-xs uppercase text-ink/55">
                  <tr>
                    <th className="px-3 py-2 font-medium">Time</th>
                    <th className="px-3 py-2 font-medium">Actor</th>
                    <th className="px-3 py-2 font-medium">Action</th>
                    <th className="px-3 py-2 font-medium">Target</th>
                    <th className="px-3 py-2 font-medium">Diff</th>
                  </tr>
                </thead>
                <tbody>
                  {items.map((entry) => (
                    <tr key={entry.id} className="border-t border-line align-top">
                      <td className="whitespace-nowrap px-3 py-3 text-ink/70">{formatDate(entry.occurred_at)}</td>
                      <td className="px-3 py-3 font-mono text-xs">{entry.actor}</td>
                      <td className="px-3 py-3">
                        <Badge variant="accent">{entry.action}</Badge>
                      </td>
                      <td className="px-3 py-3 font-mono text-xs text-ink/70">
                        {entry.target_type}:{entry.target_id.slice(0, 8)}
                      </td>
                      <td className="max-w-md px-3 py-3 font-mono text-xs text-ink/65">{entry.diff ? JSON.stringify(entry.diff) : ""}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          ) : null}

          <div className="mt-4 flex items-center justify-end gap-2">
            <Button type="button" variant="secondary" size="sm" onClick={() => setOffset(Math.max(0, offset - PAGE_SIZE))} disabled={offset === 0}>
              <ChevronLeft className="h-4 w-4" aria-hidden="true" />
              Prev
            </Button>
            <Button type="button" variant="secondary" size="sm" onClick={() => setOffset(offset + PAGE_SIZE)} disabled={items.length < PAGE_SIZE}>
              Next
              <ChevronRight className="h-4 w-4" aria-hidden="true" />
            </Button>
          </div>
        </CardContent>
      </Card>
    </div>
  );
}

function formatDate(value: string): string {
  const date = new Date(value);
  return Number.isNaN(date.getTime()) ? value : date.toLocaleString();
}
