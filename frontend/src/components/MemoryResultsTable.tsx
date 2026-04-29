import { flexRender, getCoreRowModel, useReactTable, type ColumnDef } from "@tanstack/react-table";
import { Pin, PinOff } from "lucide-react";
import { useMemo } from "react";
import { useNavigate } from "react-router-dom";

import type { MemoryEntity, MemoryUnit } from "../api/types";
import { formatDateTime, formatScore, previewText } from "../lib/format";
import { cn } from "../lib/utils";
import { Badge } from "./ui/badge";
import { Button } from "./ui/button";
import { Skeleton } from "./ui/skeleton";
import { EntityChip } from "./EntityChip";

export type MemoryRow = MemoryUnit & {
  rank?: number;
  resultScore?: number;
};

type MemoryResultsTableProps = {
  rows: MemoryRow[];
  loading: boolean;
  pendingMemoryIds: string[];
  onTogglePinned?: (memory: MemoryUnit) => void;
  showPinControls?: boolean;
};

export function MemoryResultsTable({ rows, loading, pendingMemoryIds, onTogglePinned, showPinControls = true }: MemoryResultsTableProps) {
  const navigate = useNavigate();
  const pendingIds = useMemo(() => new Set(pendingMemoryIds), [pendingMemoryIds]);
  const columns = useMemo<ColumnDef<MemoryRow>[]>(
    () => {
      const tableColumns: ColumnDef<MemoryRow>[] = [
      {
        id: "rank",
        header: "#",
        cell: ({ row }) => {
          const rank = row.original.rank;
          return rank ? <span className="w-10 text-ink/50">{rank}</span> : null;
        },
        meta: { className: "w-10" },
      },
      {
        id: "content",
        header: "Memory",
        cell: ({ row }) => {
          const memory = row.original;
          return (
            <div className="min-w-[18rem] space-y-2">
              <div className="flex flex-wrap items-center gap-2">
                <Badge variant={memory.memory_type === "semantic" ? "teal" : "rust"}>{memoryTypeLabel(memory)}</Badge>
                {memory.memory_type === "semantic" && memory.corroboration_count > 1 ? (
                  <Badge variant="purple" title={`${memory.source_episode_ids.length || memory.corroboration_count} source episodes`}>
                    ⬡ {memory.corroboration_count} sources
                  </Badge>
                ) : null}
                {memory.scope_visibility === "workspace" ? <Badge variant="green">Workspace Pool</Badge> : null}
                {memory.pinned ? <Badge variant="amber">Pinned</Badge> : null}
              </div>
              <p className="text-sm font-medium text-ink">{previewText(memory.content)}</p>
            </div>
          );
        },
      },
      {
        id: "scores",
        header: "Scores",
        cell: ({ row }) => {
          const memory = row.original;
          return (
            <div className="grid min-w-[8rem] gap-1 text-xs text-ink/65">
              <span>Importance {formatScore(memory.importance_score)}</span>
              <span>Decay {formatScore(memory.decay_score)}</span>
              {memory.resultScore !== undefined ? <span>Match {formatScore(memory.resultScore)}</span> : null}
            </div>
          );
        },
      },
      {
        id: "entities",
        header: "Entities",
        cell: ({ row }) => <EntityTagList memory={row.original} />,
      },
      {
        id: "updated",
        header: "Updated",
        cell: ({ row }) => <span className="whitespace-nowrap text-xs text-ink/65">{formatDateTime(row.original.updated_at)}</span>,
      },
      ];

      if (showPinControls) {
        tableColumns.push({
        id: "pin",
        header: "Pin",
        cell: ({ row }) => {
          const memory = row.original;
          const Icon = memory.pinned ? PinOff : Pin;
          return (
            <Button
              type="button"
              variant="ghost"
              size="icon"
              data-testid={`pin-toggle-${memory.id}`}
              aria-label={memory.pinned ? "Unpin memory" : "Pin memory"}
              disabled={pendingIds.has(memory.id)}
              onClick={(event) => {
                event.stopPropagation();
                onTogglePinned?.(memory);
              }}
            >
              <Icon className="h-4 w-4" aria-hidden="true" />
            </Button>
          );
        },
        });
      }

      return tableColumns;
    },
    [onTogglePinned, pendingIds, showPinControls],
  );
  const table = useReactTable({ data: rows, columns, getCoreRowModel: getCoreRowModel() });

  if (loading) {
    return <MemoryTableSkeleton />;
  }

  return (
    <div className="overflow-hidden rounded-lg border border-line bg-white">
      <div className="thin-scrollbar overflow-x-auto">
        <table className="w-full min-w-[760px] border-collapse text-left">
          <thead className="border-b border-line bg-soft/80 text-xs font-semibold uppercase text-ink/55">
            {table.getHeaderGroups().map((headerGroup) => (
              <tr key={headerGroup.id}>
                {headerGroup.headers.map((header) => (
                  <th key={header.id} className="px-4 py-3">
                    {header.isPlaceholder ? null : flexRender(header.column.columnDef.header, header.getContext())}
                  </th>
                ))}
              </tr>
            ))}
          </thead>
          <tbody>
            {table.getRowModel().rows.map((row) => (
              <tr
                key={row.id}
                tabIndex={0}
                role="button"
                data-testid="memory-result-row"
                data-memory-id={row.original.id}
                className="cursor-pointer border-b border-line/80 transition last:border-b-0 hover:bg-soft/70 focus:bg-soft focus:outline-none"
                onClick={() => navigate(`/memory/${row.original.id}`)}
                onKeyDown={(event) => {
                  if (event.key === "Enter" || event.key === " ") {
                    event.preventDefault();
                    navigate(`/memory/${row.original.id}`);
                  }
                }}
              >
                {row.getVisibleCells().map((cell) => (
                  <td key={cell.id} className="px-4 py-4 align-middle">
                    {flexRender(cell.column.columnDef.cell, cell.getContext())}
                  </td>
                ))}
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </div>
  );
}

function memoryTypeLabel(memory: MemoryUnit): string {
  return memory.memory_type === "semantic" ? "Semantic" : "Episodic";
}

function EntityTagList({ memory }: { memory: MemoryUnit }) {
  const entities = memory.entities ?? [];
  const tagsAsEntities: MemoryEntity[] = memory.tags.slice(0, 4).map((tag) => ({ entity_type: "topic", value: tag }));
  const visible = entities.length > 0 ? entities.slice(0, 4) : tagsAsEntities;

  if (visible.length === 0) {
    return <span className="text-xs text-ink/45">Awaiting tags</span>;
  }

  return (
    <div className="flex max-w-[15rem] flex-wrap gap-1.5">
      {visible.map((entity) => (
        <EntityChip key={`${entity.entity_type}:${entity.value}`} entity={entity} />
      ))}
      {entities.length > 4 || memory.tags.length > 4 ? <Badge variant="muted">+more</Badge> : null}
    </div>
  );
}

function MemoryTableSkeleton() {
  return (
    <div className="rounded-lg border border-line bg-white p-4">
      {Array.from({ length: 6 }, (_, index) => (
        <div key={index} className={cn("grid gap-4 py-4 md:grid-cols-[1fr_9rem_14rem_8rem_3rem]", index > 0 && "border-t border-line")}>
          <div className="space-y-2">
            <Skeleton className="h-4 w-28" />
            <Skeleton className="h-4 w-full" />
          </div>
          <div className="space-y-2">
            <Skeleton className="h-3 w-24" />
            <Skeleton className="h-3 w-20" />
          </div>
          <Skeleton className="h-8 w-full" />
          <Skeleton className="h-4 w-24" />
          <Skeleton className="h-9 w-9" />
        </div>
      ))}
    </div>
  );
}