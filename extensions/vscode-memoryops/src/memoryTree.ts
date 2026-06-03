import * as vscode from "vscode";

import { MemoryListResponse, MemorySearchResult, MemoryUnit } from "./client";
import { firstLine, truncate } from "./markdown";

export type MemoryTreeNode = MemoryItem | MessageItem | LoadMoreItem;

export class MemoryTreeProvider implements vscode.TreeDataProvider<MemoryTreeNode> {
  private readonly onDidChangeTreeDataEmitter = new vscode.EventEmitter<MemoryTreeNode | undefined | null | void>();
  readonly onDidChangeTreeData = this.onDidChangeTreeDataEmitter.event;

  private memories: MemorySearchResult[] = [];
  private message = "Search or refresh MemoryOps memories.";
  private mode: "recent" | "search" | "retrieval" | "message" | "error" = "message";
  private recentTotal = 0;

  private filterPinned: boolean | undefined = undefined;
  private filterType: "episodic" | "semantic" | undefined = undefined;
  private sortField: "importance_score" | "decay_score" | "relevance_score" | "updated_at" | "created_at" = "updated_at";
  private sortDirection: "asc" | "desc" = "desc";

  getFilterPinned(): boolean | undefined { return this.filterPinned; }
  setFilterPinned(value: boolean | undefined): void { this.filterPinned = value; }

  getFilterType(): "episodic" | "semantic" | undefined { return this.filterType; }
  setFilterType(value: "episodic" | "semantic" | undefined): void { this.filterType = value; }

  getSortField(): "importance_score" | "decay_score" | "relevance_score" | "updated_at" | "created_at" { return this.sortField; }
  setSortField(value: "importance_score" | "decay_score" | "relevance_score" | "updated_at" | "created_at"): void { this.sortField = value; }

  getSortDirection(): "asc" | "desc" { return this.sortDirection; }
  setSortDirection(value: "asc" | "desc"): void { this.sortDirection = value; }

  setRecentMemories(response: MemoryListResponse, options: { append?: boolean } = {}): void {
    const nextMemories = response.items.map((memory) => ({ ...memory }));
    this.mode = "recent";
    this.memories = options.append ? mergeMemories(this.memories, nextMemories) : nextMemories;
    this.recentTotal = response.total;
    this.message = this.memories.length > 0
      ? `Showing ${this.memories.length} of ${response.total} memories.`
      : "No memories returned.";
    this.refresh();
  }

  setSearchResults(results: MemorySearchResult[], query: string): void {
    this.mode = "search";
    this.recentTotal = 0;
    this.memories = results;
    this.message = results.length > 0
      ? `Showing ${results.length} matches for ${query}.`
      : `No matches for ${query}.`;
    this.refresh();
  }

  setRetrievedMemories(memories: MemoryUnit[], title: string): void {
    this.mode = "retrieval";
    this.recentTotal = 0;
    this.memories = memories.map((memory) => ({ ...memory }));
    this.message = memories.length > 0
      ? `Showing ${memories.length} retrieved memories.`
      : title;
    this.refresh();
  }

  setError(message: string): void {
    this.mode = "error";
    this.recentTotal = 0;
    this.memories = [];
    this.message = message;
    this.refresh();
  }

  setMessage(message: string): void {
    this.mode = "message";
    this.recentTotal = 0;
    this.memories = [];
    this.message = message;
    this.refresh();
  }

  updateMemory(memory: MemoryUnit): void {
    if (!memory.id) {
      return;
    }

    this.memories = this.memories.map((current) => current.id === memory.id
      ? { ...current, ...memory }
      : current);
    this.refresh();
  }

  removeMemory(id: string): void {
    this.memories = this.memories.filter((memory) => memory.id !== id);
    if (this.mode === "recent" && this.recentTotal > 0) {
      this.recentTotal = Math.max(this.memories.length, this.recentTotal - 1);
      if (this.memories.length === 0) {
        this.message = "No memories returned.";
      }
    }
    this.refresh();
  }

  getMemories(): readonly MemorySearchResult[] {
    return this.memories;
  }

  getNextRecentOffset(): number | undefined {
    if (this.mode !== "recent" || this.memories.length >= this.recentTotal) {
      return undefined;
    }

    return this.memories.length;
  }

  getTreeItem(element: MemoryTreeNode): vscode.TreeItem {
    return element;
  }

  getChildren(element?: MemoryTreeNode): vscode.ProviderResult<MemoryTreeNode[]> {
    if (element) {
      return [];
    }

    if (this.memories.length === 0) {
      return [new MessageItem(this.message)];
    }

    const items: MemoryTreeNode[] = this.memories.map((memory) => new MemoryItem(memory));
    if (this.getNextRecentOffset() !== undefined) {
      items.push(new LoadMoreItem(this.memories.length, this.recentTotal));
    }
    return items;
  }

  private refresh(): void {
    this.onDidChangeTreeDataEmitter.fire();
  }
}

export class MemoryItem extends vscode.TreeItem {
  constructor(readonly memory: MemorySearchResult) {
    super(memoryLabel(memory), vscode.TreeItemCollapsibleState.None);

    this.id = memory.id ? `memoryops.memory.${memory.id}` : undefined;
    this.description = memoryDescription(memory);
    this.tooltip = memoryTooltip(memory);
    this.contextValue = [
      "memoryops",
      "memory",
      memory.pinned ? "pinned" : "unpinned",
      memory.memory_type ?? "unknown",
      memory.scope_visibility ?? "scoped",
      memory.deleted_at ? "deleted" : "active",
    ].join(".");
    this.iconPath = new vscode.ThemeIcon(memory.deleted_at ? "trash" : memory.pinned ? "pinned" : "database");
    this.command = {
      command: "memoryops.openMemory",
      title: "Open Memory",
      arguments: [this],
    };
  }
}

export class LoadMoreItem extends vscode.TreeItem {
  constructor(loaded: number, total: number) {
    super(`Load more memories (${loaded}/${total})`, vscode.TreeItemCollapsibleState.None);
    this.contextValue = "memoryops.loadMore";
    this.iconPath = new vscode.ThemeIcon("chevron-down");
    this.command = {
      command: "memoryops.loadMoreMemories",
      title: "Load More Memories",
    };
  }
}

export class MessageItem extends vscode.TreeItem {
  constructor(message: string) {
    super(message, vscode.TreeItemCollapsibleState.None);
    this.iconPath = new vscode.ThemeIcon("info");
    this.command = {
      command: "memoryops.refreshMemories",
      title: "Refresh Memories",
    };
  }
}

export function memoryFromCommandArgument(argument: unknown): MemorySearchResult | undefined {
  if (argument instanceof MemoryItem) {
    return argument.memory;
  }

  if (isMemory(argument)) {
    return argument;
  }

  return undefined;
}

export function memoryLabel(memory: MemoryUnit): string {
  return truncate(firstLine(memory.content ?? memory.id ?? "Memory"), 80);
}

function memoryDescription(memory: MemorySearchResult): string {
  return [
    memory.pinned ? "pinned" : undefined,
    memory.memory_type,
    memory.scope_visibility,
    memory.score !== undefined ? `score ${formatNumber(memory.score)}` : undefined,
    memory.importance_score !== undefined ? `importance ${formatNumber(memory.importance_score)}` : undefined,
    memory.updated_at ? relativeDate(memory.updated_at) : undefined,
  ].filter(Boolean).join(" - ");
}

function memoryTooltip(memory: MemorySearchResult): string {
  return [
    memory.id ? `ID: ${memory.id}` : undefined,
    memory.memory_type ? `Type: ${memory.memory_type}` : undefined,
    memory.scope_visibility ? `Visibility: ${memory.scope_visibility}` : undefined,
    memory.pinned !== undefined ? `Pinned: ${memory.pinned ? "yes" : "no"}` : undefined,
    memory.score !== undefined ? `Score: ${formatNumber(memory.score)}` : undefined,
    memory.importance_score !== undefined ? `Importance: ${formatNumber(memory.importance_score)}` : undefined,
    memory.deleted_at ? `Deleted: ${memory.deleted_at}` : undefined,
    Array.isArray(memory.tags) && memory.tags.length > 0 ? `Tags: ${memory.tags.join(", ")}` : undefined,
    "",
    truncate(memory.content ?? "No content", 1200),
  ].filter((part) => part !== undefined).join("\n");
}

function formatNumber(value: number): string {
  return value.toFixed(3).replace(/0+$/, "").replace(/\.$/, "");
}

function relativeDate(value: string): string | undefined {
  const timestamp = Date.parse(value);
  if (Number.isNaN(timestamp)) {
    return undefined;
  }

  const elapsedMs = Date.now() - timestamp;
  const elapsedMinutes = Math.round(elapsedMs / 60000);
  if (elapsedMinutes < 1) {
    return "just now";
  }
  if (elapsedMinutes < 60) {
    return `${elapsedMinutes}m ago`;
  }

  const elapsedHours = Math.round(elapsedMinutes / 60);
  if (elapsedHours < 48) {
    return `${elapsedHours}h ago`;
  }

  const elapsedDays = Math.round(elapsedHours / 24);
  return `${elapsedDays}d ago`;
}

function isMemory(value: unknown): value is MemorySearchResult {
  return typeof value === "object" && value !== null && ("content" in value || "id" in value);
}

function mergeMemories(current: MemorySearchResult[], next: MemorySearchResult[]): MemorySearchResult[] {
  const merged = new Map<string, MemorySearchResult>();

  for (const memory of [...current, ...next]) {
    if (memory.id) {
      merged.set(memory.id, memory);
      continue;
    }

    merged.set(`anonymous-${merged.size}`, memory);
  }

  return [...merged.values()];
}