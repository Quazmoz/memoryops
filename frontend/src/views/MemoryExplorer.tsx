import { ArrowDownAZ, ArrowUpAZ, ChevronDown, ChevronRight, Filter, Search, Send, Share2, Tag, X } from "lucide-react";
import { useEffect, useMemo, useState, type FormEvent } from "react";
import { Link, useSearchParams } from "react-router-dom";

import type { MemoryListParams } from "../api/memory";
import type { MemoryTypeFilter, MemoryUnit, SortDirection, SortField } from "../api/types";
import { EmptyState } from "../components/EmptyState";
import { InlineError } from "../components/InlineError";
import { MemoryResultsTable, type MemoryRow } from "../components/MemoryResultsTable";
import { Badge } from "../components/ui/badge";
import { Button } from "../components/ui/button";
import { Input } from "../components/ui/input";
import { HelpTooltip, InfoLabel, Tooltip, TooltipContent, TooltipTrigger } from "../components/ui/tooltip";
import { useMemoryList, useMemorySearch, useUpdateMemory } from "../hooks/use-memory";
import { useTags } from "../hooks/useTags";
import { cn } from "../lib/utils";
import { useAppStore } from "../store/app-store";

const sortFields: Array<{ value: string; label: string; field: SortField; direction?: SortDirection }> = [
  { value: "importance_score", label: "Importance", field: "importance_score" },
  { value: "decay_score", label: "Decay", field: "decay_score" },
  { value: "relevance_score:desc", label: "Relevance ↑", field: "relevance_score", direction: "desc" },
  { value: "relevance_score:asc", label: "Relevance ↓", field: "relevance_score", direction: "asc" },
  { value: "updated_at", label: "Updated", field: "updated_at" },
  { value: "created_at", label: "Created", field: "created_at" },
];
const FILTER_DEBOUNCE_MS = 300;

type ScopeFilterDraft = {
  agentId: string;
  userId: string;
  repo: string;
};

const emptyScopeFilter: ScopeFilterDraft = {
  agentId: "",
  userId: "",
  repo: "",
};

export function MemoryExplorer() {
  const workspaceId = useAppStore((state) => state.workspaceId);
  const [searchParams, setSearchParams] = useSearchParams();
  const initialQuery = searchParams.get("q") ?? "";
  const [query, setQuery] = useState(initialQuery);
  const [submittedQuery, setSubmittedQuery] = useState(initialQuery);
  const initialType = (searchParams.get("type") ?? "all") as MemoryTypeFilter;
  const [memoryType, setMemoryType] = useState<MemoryTypeFilter>(
    ["all", "episodic", "semantic"].includes(initialType) ? initialType : "all",
  );
  const [includeWorkspacePool, setIncludeWorkspacePool] = useState(false);
  const [pinned, setPinned] = useState(false);
  const [minImportance, setMinImportance] = useState(0);
  const [asOfDateTime, setAsOfDateTime] = useState("");
  const [scopeDraft, setScopeDraft] = useState<ScopeFilterDraft>(emptyScopeFilter);
  const [debouncedScope, setDebouncedScope] = useState<ScopeFilterDraft>(emptyScopeFilter);
  const [selectedTags, setSelectedTags] = useState<string[]>([]);
  const [tagsCollapsed, setTagsCollapsed] = useState(false);
  const [offset, setOffset] = useState(0);
  const [sortField, setSortField] = useState<SortField>("importance_score");
  const [sortDirection, setSortDirection] = useState<SortDirection>("desc");

  useEffect(() => {
    const timeoutId = window.setTimeout(() => {
      setDebouncedScope(scopeDraft);
    }, FILTER_DEBOUNCE_MS);

    return () => window.clearTimeout(timeoutId);
  }, [scopeDraft]);

  const asOfIso = useMemo(() => localDateTimeToIso(asOfDateTime), [asOfDateTime]);

  const listParams = useMemo<MemoryListParams>(
    () => {
      const params: MemoryListParams = {
        limit: 50,
        offset,
        memoryType,
        minImportance,
        agentId: debouncedScope.agentId,
        userId: debouncedScope.userId,
        repo: debouncedScope.repo,
        sort: sortField,
        direction: sortDirection,
      };

      if (asOfIso) {
        params.asOf = asOfIso;
      }

      if (pinned) {
        params.pinned = true;
      }

      return params;
    },
    [asOfIso, debouncedScope, memoryType, minImportance, offset, pinned, sortDirection, sortField],
  );
  const searchText = submittedQuery || selectedTags.join(" ");
  const searchCriteria = useMemo(() => {
    const criteria = {
      query: searchText,
      memoryType,
      pinned,
      minImportance,
      includeWorkspacePool,
      agentId: debouncedScope.agentId,
      userId: debouncedScope.userId,
      repo: debouncedScope.repo,
      tags: selectedTags,
      limit: 50,
      offset,
      ...(asOfIso ? { asOf: asOfIso } : {}),
    };

    return criteria;
  }, [asOfIso, debouncedScope, includeWorkspacePool, memoryType, minImportance, offset, pinned, searchText, selectedTags]);
  const listQuery = useMemoryList(workspaceId, listParams);
  const searchQuery = useMemorySearch(workspaceId, searchCriteria);
  const tagsQuery = useTags(workspaceId);
  const updateMemory = useUpdateMemory(workspaceId);
  const searchActive = searchText.trim().length > 0;
  const rows = useMemo(
    () => buildRows(searchActive, searchQuery.data?.results, listQuery.data?.items, sortField, sortDirection),
    [listQuery.data?.items, searchActive, searchQuery.data?.results, sortDirection, sortField],
  );
  const loading = searchActive ? searchQuery.isLoading : listQuery.isLoading;
  const error = searchActive ? searchQuery.error : listQuery.error;

  function handleSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const trimmed = query.trim();
    setOffset(0);
    setSubmittedQuery(trimmed);
    setSearchParams(trimmed ? { q: trimmed } : {});
  }

  function handleClearSearch() {
    setQuery("");
    setSubmittedQuery("");
    setOffset(0);
    setSearchParams({});
  }

  function selectMemoryType(type: MemoryTypeFilter) {
    setMemoryType(type);
    setOffset(0);
    setSearchParams((prev) => {
      const next = new URLSearchParams(prev);
      if (type === "all") {
        next.delete("type");
      } else {
        next.set("type", type);
      }
      return next;
    });
  }

  function toggleWorkspacePool() {
    setIncludeWorkspacePool((value) => !value);
    setOffset(0);
  }

  function togglePinnedFilter() {
    setPinned((value) => !value);
    setOffset(0);
  }

  function changeMinImportance(value: number) {
    setMinImportance(value);
    setOffset(0);
  }

  function changeSortField(value: string) {
    const option = sortFields.find((field) => field.value === value);
    setSortField(option?.field ?? (value as SortField));
    if (option?.direction) {
      setSortDirection(option.direction);
    }
    setOffset(0);
  }

  function toggleSortDirection() {
    setSortDirection((direction) => (direction === "asc" ? "desc" : "asc"));
    setOffset(0);
  }

  function changeScopeFilter(field: keyof ScopeFilterDraft, value: string) {
    setScopeDraft((current) => ({ ...current, [field]: value }));
    setOffset(0);
  }

  function addTagFilter(name: string) {
    setSelectedTags((current) => (current.includes(name) ? current : [...current, name]));
    setOffset(0);
  }

  function removeTagFilter(name: string) {
    setSelectedTags((current) => current.filter((tag) => tag !== name));
    setOffset(0);
  }

  function toggleTagsPanel() {
    setTagsCollapsed((value) => !value);
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
          <HelpTooltip label="Search memory" className="self-center">
            Search stored memories by meaning or keywords across the current workspace and filter set.
          </HelpTooltip>
          <div className="relative flex-1">
            <Search className="pointer-events-none absolute left-3 top-1/2 h-4 w-4 -translate-y-1/2 text-ink/40" aria-hidden="true" />
            <Input
              data-testid="memory-search-input"
              aria-label="Search memory"
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
        <div className="grid gap-4">
          <div className="grid gap-4 lg:grid-cols-[auto_auto_auto_1fr] lg:items-center">
            <div className="flex flex-wrap gap-2" aria-label="Memory type filters">
              {(["all", "episodic", "semantic"] as MemoryTypeFilter[]).map((type) => (
                <Tooltip key={type}>
                  <TooltipTrigger asChild>
                    <button
                      type="button"
                      data-testid={`filter-type-${type}`}
                      onClick={() => selectMemoryType(type)}
                      className={filterButtonClass(memoryType === type)}
                    >
                      {type}
                    </button>
                  </TooltipTrigger>
                  <TooltipContent>{memoryTypeTooltip(type)}</TooltipContent>
                </Tooltip>
              ))}
            </div>

            <Tooltip>
              <TooltipTrigger asChild>
                <button type="button" data-testid="filter-pinned" onClick={togglePinnedFilter} className={filterButtonClass(pinned)}>
                  <Filter className="h-3.5 w-3.5" aria-hidden="true" />
                  Pinned
                </button>
              </TooltipTrigger>
              <TooltipContent>Only show memories protected from normal decay and pruning.</TooltipContent>
            </Tooltip>

            <Tooltip>
              <TooltipTrigger asChild>
                <button type="button" data-testid="filter-workspace-pool" onClick={toggleWorkspacePool} className={filterButtonClass(includeWorkspacePool)}>
                  <Share2 className="h-3.5 w-3.5" aria-hidden="true" />
                  Workspace Pool
                </button>
              </TooltipTrigger>
              <TooltipContent>Workspace-visible memories are available across agents and users in this workspace, not just the original private scope.</TooltipContent>
            </Tooltip>

            <label className="grid min-w-[16rem] gap-2 text-sm text-ink/70">
              <span className="flex justify-between text-xs font-medium uppercase text-ink/45">
                <InfoLabel label="Min importance" tooltip="Hide low-priority memories below this importance threshold." />
                <span>{minImportance.toFixed(2)}</span>
              </span>
              <input
                data-testid="filter-min-importance"
                type="range"
                min="0"
                max="1"
                step="0.05"
                value={minImportance}
                onChange={(event) => changeMinImportance(Number(event.target.value))}
                className="accent-accent"
              />
            </label>
          </div>

          <div className="grid gap-3 md:grid-cols-2 xl:grid-cols-4">
            <ScopeFilterInput label="Agent ID" helpText="Filter to memories scoped to a specific agent, such as claude-code, deploy-bot, or review-agent." testId="filter-agent-id" value={scopeDraft.agentId} onChange={(value) => changeScopeFilter("agentId", value)} />
            <ScopeFilterInput label="User ID" helpText="Filter to memories attached to a specific end user or operator scope." testId="filter-user-id" value={scopeDraft.userId} onChange={(value) => changeScopeFilter("userId", value)} />
            <ScopeFilterInput label="Repo" helpText="Filter to memories tied to a specific repository, usually owner/repo." testId="filter-repo" value={scopeDraft.repo} onChange={(value) => changeScopeFilter("repo", value)} placeholder="owner/repo" />
            <label className="grid gap-1 text-xs font-medium uppercase text-ink/45">
              <InfoLabel label="As Of" tooltip="Time-travel filter. Shows memories as they would have existed at the selected point in time." />
              <div className="flex gap-2">
                <Input
                  type="datetime-local"
                  value={asOfDateTime}
                  data-testid="as-of-input"
                  onChange={(event) => {
                    setAsOfDateTime(event.target.value);
                    setOffset(0);
                  }}
                  className="normal-case"
                />
                {asOfDateTime ? (
                  <Button
                    type="button"
                    variant="ghost"
                    size="icon"
                    aria-label="Clear as of"
                    onClick={() => {
                      setAsOfDateTime("");
                      setOffset(0);
                    }}
                  >
                    <X className="h-4 w-4" aria-hidden="true" />
                  </Button>
                ) : null}
              </div>
            </label>
          </div>
        </div>

        <div className="flex flex-wrap gap-2">
          <label className="grid gap-1 text-xs font-medium uppercase text-ink/45">
            <InfoLabel label="Sort" tooltip="Choose which score or timestamp drives the primary result ordering." />
            <select
              data-testid="sort-field-select"
              value={sortSelectValue(sortField, sortDirection)}
              onChange={(event) => changeSortField(event.target.value)}
              className="h-10 rounded-md border border-line bg-white px-3 text-sm normal-case text-ink outline-none focus:border-accent focus:ring-2 focus:ring-accent/20"
            >
              {sortFields.map((field) => (
                <option key={field.value} value={field.value}>
                  {field.label}
                </option>
              ))}
            </select>
          </label>
          <Tooltip>
            <TooltipTrigger asChild>
              <Button
                type="button"
                variant="secondary"
                data-testid="sort-direction-toggle"
                aria-label="Sort direction"
                onClick={toggleSortDirection}
              >
                {sortDirection === "asc" ? <ArrowUpAZ className="h-4 w-4" aria-hidden="true" /> : <ArrowDownAZ className="h-4 w-4" aria-hidden="true" />}
                {sortDirection}
              </Button>
            </TooltipTrigger>
            <TooltipContent>Toggle whether the selected sort runs from low-to-high or high-to-low.</TooltipContent>
          </Tooltip>
        </div>
      </section>

      <section className="rounded-lg border border-line bg-white p-4">
        <button
          type="button"
          data-testid="tags-panel-toggle"
          onClick={toggleTagsPanel}
          className="flex w-full items-center justify-between gap-3 text-left text-sm font-semibold text-ink"
        >
          <span className="inline-flex items-center gap-2">
            <Tag className="h-4 w-4 text-accent-strong" aria-hidden="true" />
            <span>Tags</span>
            <HelpTooltip label="Tags panel">Browse known tags and add them as filters to narrow the explorer to specific topics or labels.</HelpTooltip>
          </span>
          {tagsCollapsed ? <ChevronRight className="h-4 w-4" aria-hidden="true" /> : <ChevronDown className="h-4 w-4" aria-hidden="true" />}
        </button>
        {!tagsCollapsed ? (
          <div className="mt-4 flex flex-wrap gap-2">
            {tagsQuery.isLoading ? <span className="text-sm text-ink/55">Loading tags</span> : null}
            {tagsQuery.error ? <span className="text-sm text-rust">Tags could not be loaded</span> : null}
            {tagsQuery.data?.tags.map((tag) => (
              <button
                key={tag.name}
                type="button"
                data-testid={`tag-pill-${tag.name}`}
                onClick={() => addTagFilter(tag.name)}
                className={tagPillClass(selectedTags.includes(tag.name))}
              >
                <span>{tag.name}</span>
                <span className="text-ink/45">{tag.count}</span>
              </button>
            ))}
            {!tagsQuery.isLoading && !tagsQuery.error && tagsQuery.data?.tags.length === 0 ? (
              <span className="text-sm text-ink/55">No tags yet</span>
            ) : null}
          </div>
        ) : null}
      </section>

      {submittedQuery ? (
        <div className="flex flex-wrap items-center gap-2 text-sm text-ink/65">
          <span>Searching for</span>
          <Badge variant="accent">{submittedQuery}</Badge>
        </div>
      ) : null}

      {selectedTags.length > 0 ? (
        <div className="flex flex-wrap items-center gap-2 text-sm text-ink/65" data-testid="active-tag-filters">
          <InfoLabel label="Active tag filters" tooltip="Current tag filters applied to the explorer query." />
          {selectedTags.map((tag) => (
            <button
              key={tag}
              type="button"
              data-testid={`active-tag-${tag}`}
              onClick={() => removeTagFilter(tag)}
              className="inline-flex h-8 items-center gap-1 rounded-md border border-accent/20 bg-accent/10 px-2 text-xs font-medium text-accent-strong transition hover:bg-accent/15 focus:outline-none focus:ring-2 focus:ring-accent"
            >
              {tag}
              <X className="h-3.5 w-3.5" aria-hidden="true" />
            </button>
          ))}
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
  if (field === "relevance_score") {
    return row.relevance_score ?? 0.5;
  }

  const timestamp = Date.parse(field === "created_at" ? row.created_at : row.updated_at);
  return Number.isNaN(timestamp) ? 0 : timestamp;
}

function sortSelectValue(field: SortField, direction: SortDirection): string {
  return field === "relevance_score" ? `${field}:${direction}` : field;
}

function filterButtonClass(active: boolean): string {
  return cn(
    "inline-flex h-10 items-center gap-2 rounded-md border px-3 text-sm font-medium capitalize transition focus:outline-none focus:ring-2 focus:ring-accent",
    active ? "border-accent bg-accent/10 text-accent-strong" : "border-line bg-white text-ink/70 hover:bg-soft",
  );
}

function tagPillClass(active: boolean): string {
  return cn(
    "inline-flex h-8 items-center gap-2 rounded-md border px-2.5 text-xs font-medium transition focus:outline-none focus:ring-2 focus:ring-accent",
    active ? "border-accent bg-accent/10 text-accent-strong" : "border-line bg-soft text-ink/70 hover:bg-accent/10 hover:text-accent-strong",
  );
}

function ScopeFilterInput({
  label,
  helpText,
  testId,
  value,
  onChange,
  placeholder,
}: {
  label: string;
  helpText: string;
  testId: string;
  value: string;
  onChange: (value: string) => void;
  placeholder?: string;
}) {
  return (
    <label className="grid gap-1 text-xs font-medium uppercase text-ink/45">
      <InfoLabel label={label} tooltip={helpText} />
      <Input data-testid={testId} value={value} onChange={(event) => onChange(event.target.value)} placeholder={placeholder} className="normal-case text-ink" />
    </label>
  );
}

function memoryTypeTooltip(type: MemoryTypeFilter): string {
  if (type === "episodic") {
    return "Short-lived event-derived memories from raw activity like commits, PRs, messages, tickets, and agent observations.";
  }
  if (type === "semantic") {
    return "Durable knowledge promoted from recurring or important episodic memories.";
  }
  return "Shows both episodic and semantic memories in one result set.";
}

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : "Memory results could not be loaded.";
}

function localDateTimeToIso(value: string): string | undefined {
  if (!value) {
    return undefined;
  }

  const date = new Date(value);
  if (Number.isNaN(date.getTime())) {
    return undefined;
  }

  return date.toISOString();
}
