import * as vscode from "vscode";

import { MemoryListResponse, MemorySearchResult, MemoryUnit } from "./client";

export type MemoryTreeNode = MemoryItem | MessageItem;

export class MemoryTreeProvider implements vscode.TreeDataProvider<MemoryTreeNode> {
  private readonly onDidChangeTreeDataEmitter = new vscode.EventEmitter<MemoryTreeNode | undefined | null | void>();
  readonly onDidChangeTreeData = this.onDidChangeTreeDataEmitter.event;

  private memories: MemorySearchResult[] = [];
  private message = "Search or refresh MemoryOps memories.";

  setRecentMemories(response: MemoryListResponse): void {
    this.memories = response.items.map((memory) => ({ ...memory }));
    this.message = response.items.length > 0
      ? `Showing ${response.items.length} of ${response.total} memories.`
      : "No memories returned.";
    this.refresh();
  }

  setSearchResults(results: MemorySearchResult[], query: string): void {
    this.memories = results;
    this.message = results.length > 0
      ? `Showing ${results.length} matches for ${query}.`
      : `No matches for ${query}.`;
    this.refresh();
  }

  setRetrievedMemories(memories: MemoryUnit[], title: string): void {
    this.memories = memories.map((memory) => ({ ...memory }));
    this.message = memories.length > 0
      ? `Showing ${memories.length} retrieved memories.`
      : title;
    this.refresh();
  }

  setError(message: string): void {
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
    this.refresh();
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

    return this.memories.map((memory) => new MemoryItem(memory));
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
    this.contextValue = memory.pinned ? "memoryops.memory.pinned" : "memoryops.memory.unpinned";
    this.iconPath = new vscode.ThemeIcon(memory.pinned ? "pinned" : "database");
    this.command = {
      command: "memoryops.openMemory",
      title: "Open Memory",
      arguments: [this],
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
    Array.isArray(memory.tags) && memory.tags.length > 0 ? `Tags: ${memory.tags.join(", ")}` : undefined,
    "",
    truncate(memory.content ?? "No content", 1200),
  ].filter((part) => part !== undefined).join("\n");
}

function firstLine(value: string): string {
  return value.split(/\r?\n/)[0] ?? value;
}

function truncate(value: string, maxLength: number): string {
  return value.length <= maxLength ? value : `${value.slice(0, maxLength - 3)}...`;
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