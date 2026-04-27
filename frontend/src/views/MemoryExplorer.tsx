import { ArrowDownAZ, ArrowUpAZ, Filter, Search, Send, X } from "lucide-react";
import { useMemo, useState, type FormEvent } from "react";
import { Link, useSearchParams } from "react-router-dom";

import type { MemoryListParams } from "../api/memory";
import type { MemoryTypeFilter, MemoryUnit, SortDirection, SortField } from "../api/types";
import { EmptyState } from "../components/EmptyState";
import { InlineError } from "../components/InlineError";
import { MemoryResultsTable, type MemoryRow } from "../components/MemoryResultsTable";
import { Badge } from "../components/ui/badge";
import { Button } from "../components/ui/button";
import { Input } from "../components/ui/input";
import { useMemoryList, useMemorySearch, useUpdateMemory } from "../hooks/use-memory";
import { cn } from "../lib/utils";
import { useAppStore } from "../store/app-store";

const sortFields: Array<{ value: SortField; label: string }> = [
  { value: "importance_score", label: "Importance" },
  { value: "decay_score", label: "Decay" },
  { value: "updated_at", label: "Updated" },
  { value: "created_at", label: "Created" },
];

export function MemoryExplorer() {
  const workspaceId = useAppStore((state) => state.workspaceId);
  const [searchParams, setSearchParams] = useSearchParams();
  const initialQuery = searchParams.get("q") ?? "";
  const [query, setQuery] = useState(initialQuery);
  const [submittedQuery, setSubmittedQuery] = useState(initialQuery);
  const [memoryType, setMemoryType] = useState<MemoryTypeFilter>("all");
  const [pinned, setPinned] = useState(false);
  const [minImportance, setMinImportance] = useState(0);
  const [sortField, setSortField] = useState<SortField>("importance_score");
  const [sortDirection, setSortDirection] = useState<SortDirection>("desc");
  const listParams = useMemo<MemoryListParams>(
    () => {
      const params: MemoryListParams = {
        limit: 50,
        offset: 0,
        memoryType,
        minImportance,
        sort: sortField,
        direction: sortDirection,
      };

      if (pinned) {
        params.pinned = true;
      }

      return params;
    },
    [memoryType, minImportance, pinned, sortDirection, sortField],
  );
  const searchCriteria = useMemo(
    () => ({
      query: submittedQuery,
      memoryType,
      pinned,
      minImportance,
      tags: [] as string[],
      limit: 50,
      offset: 0,
    }),
    [memoryType, minImportance, pinned, submittedQuery],
  );
  const listQuery = useMemoryList(workspaceId, listParams);
  const searchQuery = useMemorySearch(workspaceId, searchCriteria);
  const updateMemory = useUpdateMemory(workspaceId);
  const searchActive = submittedQuery.trim().length > 0;
  const rows = useMemo(
    () => buildRows(searchActive, searchQuery.data?.results, listQuery.data?.items, sortField, sortDirection),
    [listQuery.data?.items, searchActive, searchQuery.data?.results, sortDirection, sortField],
  );
  const loading = searchActive ? searchQuery.isLoading : listQuery.isLoading;
  const error = searchActive ? searchQuery.error : listQuery.error;

  function handleSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const trimmed = query.trim();
    setSubmittedQuery(trimmed);
    setSearchParams(trimmed ? { q: trimmed } : {});
  }

  function handleClearSearch() {
    setQuery("");
    setSubmittedQuery("");
    setSearchParams({});
  }

  function handleTogglePinned(memory: MemoryUnit) {
    updateMemory.mutate({ id: memory.id, patch: { pinned: !memory.pinned } });
  }

  return (
    <div className="mx-auto grid max-w-7xl gap-5">
      <header className="flex flex-col gap-4 xl:flex-row xl:items-end xl:justify-between">
        <div>
          <p className="text-sm font-medium text-accent-strong">Primary view</p>
          <h1 className="mt-1 text-2xl font-semibold tracking-normal text-ink">Memory Explorer</h1>
        </div>

        <form className="flex w-full max-w-2xl gap-2" onSubmit={handleSubmit}>
          <div className="relative flex-1">
            <Search className="pointer-events-none absolute left-3 top-1/2 h-4 w-4 -translate-y-1/2 text-ink/40" aria-hidden="true" />
            <Input
              data-testid="memory-search-input"
              value={query}
              onChange={(event) => setQuery(event.target.value)}
              className="pl-9"
              placeholder="Search memory"
            />
          </div>
          {submittedQuery ? (
            <Button type="button" variant="secondary" size="icon" data-testid="memory-search-clear" aria-label="Clear search" onClick={handleClearSearch}>
              <X className="h-4 w-4" aria-hidden="true" />
            </Button>
          ) : null}
          <Button type="submit" data-testid="memory-search-submit">
            <Search className="h-4 w-4" aria-hidden="true" />
            Search
          </Button>
        </form>
      </header>

      <section className="grid gap-3 rounded-lg border border-line bg-white p-4 xl:grid-cols-[1fr_auto] xl:items-end">
        <div className="grid gap-4 lg:grid-cols-[auto_auto_1fr] lg:items-center">
          <div className="flex flex-wrap gap-2" aria-label="Memory type filters">
            {(["all", "episodic", "semantic"] as MemoryTypeFilter[]).map((type) => (
              <button
                key={type}
                type="button"
                data-testid={`filter-type-${type}`}
                onClick={() => setMemoryType(type)}
                className={filterButtonClass(memoryType === type)}
              >
                {type}
              </button>
            ))}
          </div>

          <button type="button" data-testid="filter-pinned" onClick={() => setPinned((value) => !value)} className={filterButtonClass(pinned)}>
            <Filter className="h-3.5 w-3.5" aria-hidden="true" />
            Pinned
          </button>

          <label className="grid min-w-[16rem] gap-2 text-sm text-ink/70">
            <span className="flex justify-between text-xs font-medium uppercase text-ink/45">
              <span>Min importance</span>
              <span>{minImportance.toFixed(2)}</span>
            </span>
            <input
              data-testid="filter-min-importance"
              type="range"
              min="0"
              max="1"
              step="0.05"
              value={minImportance}
              onChange={(event) => setMinImportance(Number(event.target.value))}
              className="accent-accent"
            />
          </label>
        </div>

        <div className="flex flex-wrap gap-2">
          <label className="grid gap-1 text-xs font-medium uppercase text-ink/45">
            Sort
            <select
              data-testid="sort-field-select"
              value={sortField}
              onChange={(event) => setSortField(event.target.value as SortField)}
              className="h-10 rounded-md border border-line bg-white px-3 text-sm normal-case text-ink outline-none focus:border-accent focus:ring-2 focus:ring-accent/20"
            >
              {sortFields.map((field) => (
                <option key={field.value} value={field.value}>
                  {field.label}
                </option>
              ))}
            </select>
          </label>
          <Button
            type="button"
            variant="secondary"
            data-testid="sort-direction-toggle"
            onClick={() => setSortDirection((direction) => (direction === "asc" ? "desc" : "asc"))}
          >
            {sortDirection === "asc" ? <ArrowUpAZ className="h-4 w-4" aria-hidden="true" /> : <ArrowDownAZ className="h-4 w-4" aria-hidden="true" />}
            {sortDirection}
          </Button>
        </div>
      </section>

      {submittedQuery ? (
        <div className="flex flex-wrap items-center gap-2 text-sm text-ink/65">
          <span>Searching for</span>
          <Badge variant="accent">{submittedQuery}</Badge>
        </div>
      ) : null}

      {error ? <InlineError message={errorMessage(error)} /> : null}

      {!loading && !error && rows.length === 0 ? (
        <EmptyState
          title={searchActive ? "That search is quiet" : "Explorer is ready for memory"}
          message={searchActive ? "Try a broader phrase or loosen one of the filters." : "Ingest a GitHub event and the first memories will appear here with scores, tags, and pin controls."}
          action={
            <Button asChild variant="secondary">
              <Link to="/ingest" data-testid="empty-go-ingest">
                <Send className="h-4 w-4" aria-hidden="true" />
                Webhook Tester
              </Link>
            </Button>
          }
        />
      ) : null}

      {loading || rows.length > 0 ? (
        <MemoryResultsTable
          rows={rows}
          loading={loading}
          pendingMemoryIds={updateMemory.isPending && updateMemory.variables ? [updateMemory.variables.id] : []}
          onTogglePinned={handleTogglePinned}
        />
      ) : null}
    </div>
  );
}

function buildRows(
  searchActive: boolean,
  searchResults: Array<{ memory: MemoryUnit; score: number; rank: number }> | undefined,
  listItems: MemoryUnit[] | undefined,
  sortField: SortField,
  sortDirection: SortDirection,
): MemoryRow[] {
  const rows: MemoryRow[] = searchActive
    ? (searchResults ?? []).map((result) => ({ ...result.memory, rank: result.rank, resultScore: result.score }))
    : (listItems ?? []);

  return [...rows].sort((left, right) => compareRows(left, right, sortField, sortDirection));
}

function compareRows(left: MemoryRow, right: MemoryRow, field: SortField, direction: SortDirection): number {
  const multiplier = direction === "asc" ? 1 : -1;
  const leftValue = rowValue(left, field);
  const rightValue = rowValue(right, field);

  if (leftValue < rightValue) {
    return -1 * multiplier;
  }
  if (leftValue > rightValue) {
    return 1 * multiplier;
  }
  return 0;
}

function rowValue(row: MemoryRow, field: SortField): number {
  if (field === "importance_score") {
    return row.importance_score;
  }
  if (field === "decay_score") {
    return row.decay_score;
  }

  const timestamp = Date.parse(field === "created_at" ? row.created_at : row.updated_at);
  return Number.isNaN(timestamp) ? 0 : timestamp;
}

function filterButtonClass(active: boolean): string {
  return cn(
    "inline-flex h-10 items-center gap-2 rounded-md border px-3 text-sm font-medium capitalize transition focus:outline-none focus:ring-2 focus:ring-accent",
    active ? "border-accent bg-accent/10 text-accent-strong" : "border-line bg-white text-ink/70 hover:bg-soft",
  );
}

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : "Memory results could not be loaded.";
}