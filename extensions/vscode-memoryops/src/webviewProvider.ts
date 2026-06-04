import * as vscode from "vscode";
import { MemorySearchResult, MemoryUnit } from "./client";

export class MemoryWebviewViewProvider implements vscode.WebviewViewProvider {
  public static readonly viewType = "memoryops.memories";

  private _view?: vscode.WebviewView;
  private _memories: MemorySearchResult[] = [];
  private _searchQuery = "";
  private _activeTab: "all" | "episodic" | "semantic" | "pinned" = "all";
  private _statusMessage = "Search or refresh MemoryOps memories.";
  private _mode: "recent" | "search" | "retrieval" | "message" | "error" = "message";
  private _recentTotal = 0;

  private _filterPinned: boolean | undefined = undefined;
  private _filterType: "episodic" | "semantic" | undefined = undefined;
  private _sortField: "importance_score" | "decay_score" | "relevance_score" | "updated_at" | "created_at" = "updated_at";
  private _sortDirection: "asc" | "desc" = "desc";

  constructor(private readonly _extensionUri: vscode.Uri) {}

  getFilterPinned(): boolean | undefined { return this._filterPinned; }
  setFilterPinned(value: boolean | undefined): void { this._filterPinned = value; }

  getFilterType(): "episodic" | "semantic" | undefined { return this._filterType; }
  setFilterType(value: "episodic" | "semantic" | undefined): void { this._filterType = value; }

  getSortField(): "importance_score" | "decay_score" | "relevance_score" | "updated_at" | "created_at" { return this._sortField; }
  setSortField(value: "importance_score" | "decay_score" | "relevance_score" | "updated_at" | "created_at"): void { this._sortField = value; }

  getSortDirection(): "asc" | "desc" { return this._sortDirection; }
  setSortDirection(value: "asc" | "desc"): void { this._sortDirection = value; }

  getMode(): "recent" | "search" | "retrieval" | "message" | "error" { return this._mode; }

  public setMessage(message: string): void {
    this._mode = "message";
    this._recentTotal = 0;
    this._memories = [];
    this._statusMessage = message;
    this.updateWebview();
  }

  public setError(message: string): void {
    this._mode = "error";
    this._recentTotal = 0;
    this._memories = [];
    this._statusMessage = message;
    this.updateWebview();
  }

  public setRecentMemories(response: { items: MemoryUnit[], total: number }, options: { append?: boolean } = {}): void {
    const nextMemories = response.items.map((memory) => ({ ...memory }));
    this._mode = "recent";
    this._memories = options.append ? this._mergeMemories(this._memories, nextMemories) : nextMemories;
    this._recentTotal = response.total;
    this._statusMessage = this._memories.length > 0
      ? `Showing ${this._memories.length} of ${response.total} memories.`
      : "No memories returned.";
    this.updateWebview();
  }

  public setSearchResults(results: MemorySearchResult[], query: string): void {
    this._mode = "search";
    this._recentTotal = 0;
    this._memories = results;
    this._statusMessage = results.length > 0
      ? `Showing ${results.length} matches for ${query}.`
      : `No matches for ${query}.`;
    this.updateWebview();
  }

  public setRetrievedMemories(memories: MemoryUnit[], title: string): void {
    this._mode = "retrieval";
    this._recentTotal = 0;
    this._memories = memories.map((memory) => ({ ...memory }));
    this._statusMessage = memories.length > 0
      ? `Showing ${memories.length} retrieved memories.`
      : title;
    this.updateWebview();
  }

  public getNextRecentOffset(): number | undefined {
    if (this._mode !== "recent" || this._memories.length >= this._recentTotal) {
      return undefined;
    }
    return this._memories.length;
  }

  private _mergeMemories(current: MemorySearchResult[], next: MemorySearchResult[]): MemorySearchResult[] {
    const merged = new Map<string, MemorySearchResult>();
    let anonymousIndex = 0;
    for (const memory of [...current, ...next]) {
      if (memory.id) {
        merged.set(memory.id, memory);
        continue;
      }
      merged.set(`anonymous-${anonymousIndex++}`, memory);
    }
    return [...merged.values()];
  }

  public resolveWebviewView(
    webviewView: vscode.WebviewView,
    _context: vscode.WebviewViewResolveContext,
    _token: vscode.CancellationToken
  ): void {
    this._view = webviewView;

    webviewView.webview.options = {
      enableScripts: true,
      localResourceRoots: [this._extensionUri],
    };

    webviewView.webview.html = this._getHtmlForWebview(webviewView.webview);

    webviewView.webview.onDidReceiveMessage((data) => {
      switch (data.type) {
        case "ready": {
          this.refreshList();
          break;
        }
        case "search": {
          this._searchQuery = data.query;
          vscode.commands.executeCommand("memoryops.searchMemoryInline", data.query);
          break;
        }
        case "refresh": {
          vscode.commands.executeCommand("memoryops.refreshMemories");
          break;
        }
        case "openSettings": {
          vscode.commands.executeCommand("memoryops.openSettings");
          break;
        }
        case "tabChanged": {
          this._activeTab = data.tab;
          this.updateWebview();
          break;
        }
        case "pin": {
          vscode.commands.executeCommand(data.pinned ? "memoryops.pinMemory" : "memoryops.unpinMemory", data.id);
          break;
        }
        case "promote": {
          vscode.commands.executeCommand("memoryops.promoteMemory", data.id);
          break;
        }
        case "publish": {
          vscode.commands.executeCommand("memoryops.publishMemory", data.id);
          break;
        }
        case "delete": {
          vscode.commands.executeCommand("memoryops.deleteMemory", data.id);
          break;
        }
        case "copy": {
          const memory = this._memories.find((m) => m.id === data.id);
          const content = typeof data.content === "string" ? data.content : memory?.content ?? "";
          vscode.env.clipboard.writeText(content);
          vscode.window.showInformationMessage("Memory content copied to clipboard.");
          break;
        }
        case "edit": {
          vscode.commands.executeCommand("memoryops.editMemoryInline", data.id, data.field);
          break;
        }
        case "submitFeedback": {
          vscode.commands.executeCommand("memoryops.submitFeedbackInline", data.id, {
            queryId: data.queryId,
            rating: data.rating,
            comment: data.comment,
          });
          break;
        }
        case "openDetails": {
          vscode.commands.executeCommand("memoryops.openMemory", data.id);
          break;
        }
      }
    });

    // Initial load
    this.updateWebview();
  }

  public setMemories(memories: MemorySearchResult[], statusMessage?: string): void {
    this._memories = memories;
    if (statusMessage) {
      this._statusMessage = statusMessage;
    } else {
      this._statusMessage = memories.length > 0 ? `Showing ${memories.length} memories.` : "No memories returned.";
    }
    this.updateWebview();
  }

  public updateMemory(updated: MemoryUnit): void {
    this._memories = this._memories.map((m) => (m.id === updated.id ? { ...m, ...updated } : m));
    this.updateWebview();
  }

  public removeMemory(id: string): void {
    const before = this._memories.length;
    this._memories = this._memories.filter((m) => m.id !== id);
    if (this._mode === "recent" && this._memories.length < before) {
      this._recentTotal = Math.max(this._memories.length, this._recentTotal - 1);
      this._statusMessage = this._memories.length > 0
        ? `Showing ${this._memories.length} of ${this._recentTotal} memories.`
        : "No memories returned.";
    }
    this.updateWebview();
  }

  public getMemories(): MemorySearchResult[] {
    return this._memories;
  }

  public refreshList(): void {
    vscode.commands.executeCommand("memoryops.refreshMemories", { promptOnMissingConfig: false });
  }

  public updateWebview(): void {
    if (!this._view) {
      return;
    }
    this._view.webview.postMessage({
      type: "state",
      memories: this._memories,
      activeTab: this._activeTab,
      searchQuery: this._searchQuery,
      statusMessage: this._statusMessage,
    });
  }

  private _getHtmlForWebview(webview: vscode.Webview): string {
    // A nonce lets the inline <script> run under a strict Content-Security-Policy.
    // Without an explicit CSP, stricter editor environments block the inline
    // script entirely — which previously left the view stuck on "Loading..." and
    // made every button (refresh/settings/card actions) a no-op.
    const nonce = getNonce();
    const csp = [
      "default-src 'none'",
      `img-src ${webview.cspSource} https: data:`,
      `style-src ${webview.cspSource} 'unsafe-inline'`,
      `font-src ${webview.cspSource}`,
      `script-src 'nonce-${nonce}'`,
    ].join("; ");

    return `<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <meta http-equiv="Content-Security-Policy" content="${csp}">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <title>MemoryOps</title>
  <style>
    :root {
      --accent-purple: #8b5cf6;
      --accent-blue: #3b82f6;
      --accent-pink: #ec4899;
      --accent-amber: #f59e0b;
      --card-bg: rgba(30, 41, 59, 0.45);
      --card-border: rgba(255, 255, 255, 0.08);
      --card-hover-border: rgba(139, 92, 246, 0.3);
      --card-glow: rgba(139, 92, 246, 0.05);
      --font-family: var(--vscode-font-family, -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, Helvetica, Arial, sans-serif);
    }

    body {
      padding: 12px;
      font-family: var(--font-family);
      color: var(--vscode-editor-foreground);
      background-color: var(--vscode-sideBar-background);
      margin: 0;
      box-sizing: border-box;
      font-size: 12px;
    }

    /* Scrollbar Styling */
    ::-webkit-scrollbar {
      width: 6px;
      height: 6px;
    }
    ::-webkit-scrollbar-track {
      background: transparent;
    }
    ::-webkit-scrollbar-thumb {
      background: var(--vscode-scrollbarSlider-background, rgba(255, 255, 255, 0.1));
      border-radius: 3px;
    }
    ::-webkit-scrollbar-thumb:hover {
      background: var(--vscode-scrollbarSlider-activeBackground, rgba(255, 255, 255, 0.2));
    }

    .header {
      display: flex;
      flex-direction: column;
      gap: 10px;
      margin-bottom: 14px;
      position: sticky;
      top: 0;
      background-color: var(--vscode-sideBar-background);
      z-index: 10;
      padding-bottom: 8px;
      border-bottom: 1px solid var(--vscode-sideBar-border, rgba(255, 255, 255, 0.1));
    }

    .toolbar {
      display: flex;
      align-items: center;
      gap: 8px;
    }

    .search-container {
      position: relative;
      flex-grow: 1;
    }

    .search-input {
      width: 100%;
      background-color: var(--vscode-input-background);
      color: var(--vscode-input-foreground);
      border: 1px solid var(--vscode-input-border, transparent);
      padding: 6px 26px 6px 8px;
      border-radius: 4px;
      box-sizing: border-box;
      outline: none;
      font-size: 12px;
    }

    .search-input:focus {
      border-color: var(--vscode-focusBorder);
    }

    .search-clear {
      position: absolute;
      right: 6px;
      top: 50%;
      transform: translateY(-50%);
      background: none;
      border: none;
      color: var(--vscode-input-foreground);
      opacity: 0.5;
      cursor: pointer;
      padding: 2px;
      display: none;
    }

    .search-clear:hover {
      opacity: 1;
    }

    .icon-button {
      background: var(--vscode-button-secondaryBackground, rgba(255, 255, 255, 0.05));
      color: var(--vscode-button-secondaryForeground, var(--vscode-editor-foreground));
      border: 1px solid var(--vscode-button-border, rgba(255, 255, 255, 0.1));
      padding: 6px;
      border-radius: 4px;
      cursor: pointer;
      display: flex;
      align-items: center;
      justify-content: center;
      transition: all 0.2s ease;
    }

    .icon-button:hover {
      background: var(--vscode-button-secondaryHoverBackground, rgba(255, 255, 255, 0.1));
      border-color: var(--accent-purple);
    }

    .tabs {
      display: flex;
      background: rgba(0, 0, 0, 0.2);
      border-radius: 6px;
      padding: 2px;
      gap: 2px;
    }

    .tab {
      flex: 1;
      text-align: center;
      padding: 5px 2px;
      border-radius: 4px;
      cursor: pointer;
      transition: all 0.2s ease;
      font-weight: 500;
      opacity: 0.6;
      border: none;
      background: transparent;
      color: var(--vscode-editor-foreground);
      font-size: 11px;
    }

    .tab:hover {
      opacity: 0.9;
      background: rgba(255, 255, 255, 0.03);
    }

    .tab.active {
      opacity: 1;
      background: var(--vscode-button-background);
      color: var(--vscode-button-foreground);
      box-shadow: 0 1px 3px rgba(0, 0, 0, 0.2);
    }

    .status-bar {
      font-size: 10px;
      opacity: 0.6;
      padding: 2px 4px;
    }

    .cards-list {
      display: flex;
      flex-direction: column;
      gap: 12px;
      padding-bottom: 20px;
    }

    .card {
      background: var(--card-bg);
      border: 1px solid var(--card-border);
      border-radius: 8px;
      padding: 10px;
      position: relative;
      transition: all 0.3s cubic-bezier(0.25, 0.8, 0.25, 1);
      display: flex;
      flex-direction: column;
      gap: 8px;
      backdrop-filter: blur(10px);
      box-shadow: 0 4px 12px rgba(0, 0, 0, 0.08);
      overflow: hidden;
    }

    .card:hover {
      border-color: var(--card-hover-border);
      box-shadow: 0 6px 16px var(--card-glow);
      transform: translateY(-1px);
    }

    .card-header {
      display: flex;
      align-items: center;
      justify-content: space-between;
      gap: 8px;
    }

    .card-badges {
      display: flex;
      align-items: center;
      gap: 6px;
    }

    .badge {
      font-size: 9px;
      font-weight: 600;
      text-transform: uppercase;
      padding: 2px 6px;
      border-radius: 4px;
      letter-spacing: 0.3px;
    }

    .badge-episodic {
      background: rgba(236, 72, 153, 0.15);
      color: var(--accent-pink);
      border: 1px solid rgba(236, 72, 153, 0.25);
    }

    .badge-semantic {
      background: rgba(139, 92, 246, 0.15);
      color: var(--accent-purple);
      border: 1px solid rgba(139, 92, 246, 0.25);
    }

    .badge-workspace {
      background: rgba(59, 130, 246, 0.15);
      color: var(--accent-blue);
      border: 1px solid rgba(59, 130, 246, 0.25);
    }

    .card-header-right {
      display: flex;
      align-items: center;
      gap: 8px;
    }

    .card-date {
      font-size: 10px;
      opacity: 0.5;
    }

    .pin-btn {
      background: none;
      border: none;
      cursor: pointer;
      color: var(--vscode-editor-foreground);
      opacity: 0.4;
      padding: 0;
      transition: all 0.2s ease;
      display: flex;
      align-items: center;
    }

    .pin-btn:hover {
      opacity: 0.9;
      color: var(--accent-amber);
    }

    .pin-btn.pinned {
      opacity: 1;
      color: var(--accent-amber);
    }

    .card-content {
      font-size: 11.5px;
      line-height: 1.45;
      color: var(--vscode-editor-foreground);
      white-space: pre-wrap;
      max-height: 90px;
      overflow: hidden;
      position: relative;
      transition: max-height 0.3s ease;
    }

    .card-content.expanded {
      max-height: 1000px;
    }

    .content-fade {
      position: absolute;
      bottom: 0;
      left: 0;
      right: 0;
      height: 25px;
      background: linear-gradient(to top, var(--vscode-sideBar-background), transparent);
      pointer-events: none;
      display: block;
    }

    .card-content.expanded .content-fade {
      display: none;
    }

    .read-more-btn {
      background: none;
      border: none;
      color: var(--vscode-textLink-foreground);
      cursor: pointer;
      font-size: 10px;
      font-weight: 500;
      align-self: flex-start;
      padding: 0;
      margin-top: -2px;
      outline: none;
    }

    .read-more-btn:hover {
      text-decoration: underline;
    }

    .tags-container {
      display: flex;
      flex-wrap: wrap;
      gap: 4px;
      margin-top: 2px;
    }

    .tag {
      background: rgba(255, 255, 255, 0.04);
      color: var(--vscode-editor-foreground);
      border: 1px solid rgba(255, 255, 255, 0.05);
      border-radius: 3px;
      padding: 1px 5px;
      font-size: 9.5px;
      opacity: 0.7;
    }

    .meters {
      display: flex;
      flex-direction: column;
      gap: 4px;
      margin-top: 4px;
      background: rgba(0, 0, 0, 0.15);
      padding: 6px;
      border-radius: 6px;
    }

    .meter-row {
      display: flex;
      align-items: center;
      justify-content: space-between;
      font-size: 9.5px;
      gap: 10px;
    }

    .meter-label {
      opacity: 0.6;
      width: 55px;
    }

    .meter-bar-container {
      flex-grow: 1;
      height: 4px;
      background: rgba(255, 255, 255, 0.08);
      border-radius: 2px;
      overflow: hidden;
      position: relative;
    }

    .meter-bar {
      height: 100%;
      border-radius: 2px;
    }

    .meter-bar-importance {
      background: linear-gradient(to right, var(--accent-blue), var(--accent-purple));
    }

    .meter-bar-relevance {
      background: linear-gradient(to right, var(--accent-purple), var(--accent-pink));
    }

    .meter-val {
      font-weight: 600;
      width: 30px;
      text-align: right;
    }

    /* Actions Toolbar */
    .card-footer {
      display: flex;
      align-items: center;
      justify-content: space-between;
      margin-top: 6px;
      border-top: 1px solid rgba(255, 255, 255, 0.04);
      padding-top: 6px;
    }

    .footer-left {
      display: flex;
      align-items: center;
      gap: 8px;
    }

    .footer-right {
      display: flex;
      align-items: center;
      gap: 4px;
    }

    .action-btn {
      background: none;
      border: none;
      color: var(--vscode-editor-foreground);
      opacity: 0.5;
      cursor: pointer;
      padding: 4px;
      border-radius: 3px;
      display: flex;
      align-items: center;
      justify-content: center;
      transition: all 0.2s ease;
    }

    .action-btn:hover {
      opacity: 0.9;
      background: rgba(255, 255, 255, 0.06);
    }

    .action-btn-danger:hover {
      color: #ef4444;
      background: rgba(239, 68, 68, 0.1);
    }

    .action-btn-accent:hover {
      color: var(--accent-purple);
      background: rgba(139, 92, 246, 0.1);
    }

    /* Feedback Layout */
    .feedback-trigger-row {
      display: flex;
      align-items: center;
      gap: 6px;
    }

    .feedback-trigger-btn {
      background: none;
      border: none;
      color: var(--vscode-editor-foreground);
      opacity: 0.4;
      cursor: pointer;
      padding: 3px;
      border-radius: 3px;
      display: flex;
      align-items: center;
      font-size: 10px;
    }

    .feedback-trigger-btn:hover {
      opacity: 0.9;
    }

    .feedback-trigger-btn.active {
      opacity: 1;
    }

    .feedback-trigger-btn.thumbs-up:hover,
    .feedback-trigger-btn.thumbs-up.active {
      color: #10b981;
    }

    .feedback-trigger-btn.thumbs-down:hover,
    .feedback-trigger-btn.thumbs-down.active {
      color: #ef4444;
    }

    .feedback-comment-box {
      display: none;
      flex-direction: column;
      gap: 4px;
      margin-top: 6px;
      background: rgba(0, 0, 0, 0.2);
      padding: 6px;
      border-radius: 4px;
    }

    .feedback-comment-input {
      background: var(--vscode-input-background);
      color: var(--vscode-input-foreground);
      border: 1px solid var(--vscode-input-border, transparent);
      padding: 4px;
      border-radius: 3px;
      font-size: 10.5px;
      outline: none;
      width: 100%;
      box-sizing: border-box;
      resize: vertical;
    }

    .feedback-comment-input:focus {
      border-color: var(--vscode-focusBorder);
    }

    .feedback-submit-row {
      display: flex;
      justify-content: flex-end;
      gap: 4px;
    }

    .feedback-submit-btn {
      background: var(--vscode-button-background);
      color: var(--vscode-button-foreground);
      border: none;
      padding: 3px 8px;
      border-radius: 3px;
      font-size: 9px;
      cursor: pointer;
    }

    .feedback-submit-btn:hover {
      background: var(--vscode-button-hoverBackground);
    }

    .feedback-cancel-btn {
      background: none;
      border: none;
      color: var(--vscode-editor-foreground);
      opacity: 0.5;
      padding: 3px 8px;
      font-size: 9px;
      cursor: pointer;
    }

    .feedback-cancel-btn:hover {
      opacity: 0.8;
    }

    .empty-state {
      text-align: center;
      padding: 30px 10px;
      opacity: 0.5;
      display: flex;
      flex-direction: column;
      align-items: center;
      gap: 10px;
    }

    .svg-icon {
      width: 14px;
      height: 14px;
      fill: currentColor;
    }
  </style>
</head>
<body>
  <div class="header">
    <div class="toolbar">
      <div class="search-container">
        <input type="text" class="search-input" id="search" placeholder="Search memories..." />
        <button class="search-clear" id="search-clear" title="Clear search">✕</button>
      </div>
      <button class="icon-button" id="refresh-btn" title="Refresh Memories">
        <svg class="svg-icon" viewBox="0 0 16 16"><path d="M13.6 2.3C12.2.9 10.2.1 8 .1 3.6.1 0 3.7 0 8.1s3.6 8 8 8c3.2 0 6-1.9 7.2-4.8l-1.3-.5c-1 2.3-3.2 3.8-5.9 3.8-3.6 0-6.5-2.9-6.5-6.5S4.4 1.6 8 1.6c1.8 0 3.4.7 4.6 1.9l-2.1 2.1h5.6V0L13.6 2.3z"/></svg>
      </button>
      <button class="icon-button" id="settings-btn" title="Open Settings">
        <svg class="svg-icon" viewBox="0 0 16 16"><path d="M9.1 1.006A1.5 1.5 0 0 0 7.728.016l-.28-.01a1.5 1.5 0 0 0-1.425.99L5.6 2.222a6.767 6.767 0 0 0-1.572.909l-1.411-.798a1.5 1.5 0 0 0-1.986.386l-.16.232a1.5 1.5 0 0 0 .193 1.983L1.75 6.02a6.772 6.772 0 0 0 .041 1.81l-1.127 1.054a1.5 1.5 0 0 0-.27 1.974l.142.242a1.5 1.5 0 0 0 1.932.482l1.455-.722a6.77 6.77 0 0 0 1.517.997l.386 1.554a1.5 1.5 0 0 0 1.396 1.12l.278.01a1.5 1.5 0 0 0 1.442-.962l.462-1.533c.548-.22 1.056-.523 1.508-.897l1.43.76a1.5 1.5 0 0 0 1.974-.356l.169-.225a1.5 1.5 0 0 0-.154-1.996l-1.077-1.107a6.776 6.776 0 0 0 .012-1.802l1.171-1.006a1.5 1.5 0 0 0 .344-1.963l-.125-.251a1.5 1.5 0 0 0-1.905-.584l-1.48.667A6.772 6.772 0 0 0 9.5 3.328l-.4-2.322zM8 10a2 2 0 1 1 0-4 2 2 0 0 1 0 4z"/></svg>
      </button>
    </div>
    <div class="tabs">
      <button class="tab active" data-tab="all">All</button>
      <button class="tab" data-tab="episodic">Episodic</button>
      <button class="tab" data-tab="semantic">Semantic</button>
      <button class="tab" data-tab="pinned">Pinned</button>
    </div>
    <div class="status-bar" id="status">Loading...</div>
  </div>

  <div class="cards-list" id="cards-container">
    <div class="empty-state">
      <svg class="svg-icon" style="width: 32px; height: 32px; opacity: 0.3;" viewBox="0 0 16 16"><path d="M8 0a8 8 0 1 0 0 16A8 8 0 0 0 8 0zm1 12H7v-2h2v2zm0-3H7V4h2v5z"/></svg>
      <span>No memories loaded yet. Make sure MemoryOps backend is running and settings are complete.</span>
      <button class="feedback-submit-btn" id="empty-settings-btn" style="margin-top: 8px; font-size: 11px; padding: 4px 12px;">Open Settings</button>
    </div>
  </div>

  <script nonce="${nonce}">
    const vscode = acquireVsCodeApi();
    window.vscode = vscode;

    // DOM Cache
    const cardsContainer = document.getElementById("cards-container");
    const searchInput = document.getElementById("search");
    const searchClear = document.getElementById("search-clear");
    const refreshBtn = document.getElementById("refresh-btn");
    const settingsBtn = document.getElementById("settings-btn");
    const statusBar = document.getElementById("status");
    const tabs = document.querySelectorAll(".tab");

    // Local State
    let state = {
      memories: [],
      activeTab: "all",
      searchQuery: "",
      statusMessage: "Loading..."
    };

    // Toolbar buttons (wired here rather than via inline onclick, which a strict CSP blocks)
    refreshBtn.addEventListener("click", () => vscode.postMessage({ type: "refresh" }));
    settingsBtn.addEventListener("click", () => vscode.postMessage({ type: "openSettings" }));
    const emptySettingsBtn = document.getElementById("empty-settings-btn");
    if (emptySettingsBtn) {
      emptySettingsBtn.addEventListener("click", () => vscode.postMessage({ type: "openSettings" }));
    }

    // Handle messages from Extension Host. Register BEFORE posting "ready" so the
    // first state push from the host can never be missed (was: stuck on "Loading...").
    window.addEventListener("message", event => {
      const message = event.data;
      if (message.type === "state") {
        state.memories = message.memories || [];
        state.activeTab = message.activeTab || "all";
        state.searchQuery = message.searchQuery || "";
        state.statusMessage = message.statusMessage || "";

        // Sync Search Box
        if (document.activeElement !== searchInput) {
          searchInput.value = state.searchQuery;
        }
        searchClear.style.display = state.searchQuery ? "block" : "none";

        // Sync Tabs
        tabs.forEach(tab => {
          if (tab.getAttribute("data-tab") === state.activeTab) {
            tab.classList.add("active");
          } else {
            tab.classList.remove("active");
          }
        });

        // Sync Status Bar
        statusBar.textContent = state.statusMessage;

        // Render Memories
        renderMemories();
      }
    });

    // Event Listeners

    // Realtime Search
    let searchTimeout;
    searchInput.addEventListener("input", (e) => {
      const val = e.target.value;
      searchClear.style.display = val ? "block" : "none";
      clearTimeout(searchTimeout);
      searchTimeout = setTimeout(() => {
        vscode.postMessage({ type: "search", query: val });
      }, 300);
    });

    searchClear.addEventListener("click", () => {
      searchInput.value = "";
      searchClear.style.display = "none";
      vscode.postMessage({ type: "search", query: "" });
      searchInput.focus();
    });

    // Tab switcher
    tabs.forEach(tab => {
      tab.addEventListener("click", () => {
        const selectedTab = tab.getAttribute("data-tab");
        vscode.postMessage({ type: "tabChanged", tab: selectedTab });
      });
    });

    // Delegated click handling for dynamically rendered cards. Inline onclick
    // attributes are blocked under the CSP, so all card actions route through here.
    cardsContainer.addEventListener("click", (e) => {
      const actionEl = e.target.closest("[data-action]");
      if (actionEl) {
        e.stopPropagation();
        const id = actionEl.dataset.id;
        switch (actionEl.dataset.action) {
          case "open-settings-hint": vscode.postMessage({ type: "openSettings" }); break;
          case "pin": vscode.postMessage({ type: "pin", id, pinned: actionEl.dataset.pinned === "true" }); break;
          case "promote": vscode.postMessage({ type: "promote", id }); break;
          case "publish": vscode.postMessage({ type: "publish", id }); break;
          case "copy": vscode.postMessage({ type: "copy", id }); break;
          case "edit": vscode.postMessage({ type: "edit", id, field: "all" }); break;
          case "delete": vscode.postMessage({ type: "delete", id }); break;
          case "read-more": toggleReadMore(id); break;
          case "feedback-up": toggleFeedbackPanel(id, 1); break;
          case "feedback-down": toggleFeedbackPanel(id, -1); break;
          case "feedback-cancel": closeFeedbackPanel(id); break;
          case "feedback-submit": submitFeedback(id); break;
        }
        return;
      }

      // Clicks inside the feedback editor must not open the detail view.
      if (e.target.closest("[data-no-card-open]")) {
        return;
      }

      const card = e.target.closest(".card");
      if (card && card.dataset.id) {
        vscode.postMessage({ type: "openDetails", id: card.dataset.id });
      }
    });

    // Initialize — post AFTER listeners are wired so no host message is dropped.
    vscode.postMessage({ type: "ready" });

    // Rendering Logic
    function renderMemories() {
      // Filter list locally based on tabs
      let filtered = state.memories;
      if (state.activeTab === "episodic") {
        filtered = state.memories.filter(m => m.memory_type === "episodic");
      } else if (state.activeTab === "semantic") {
        filtered = state.memories.filter(m => m.memory_type === "semantic");
      } else if (state.activeTab === "pinned") {
        filtered = state.memories.filter(m => m.pinned);
      }

      if (filtered.length === 0) {
        const hasSettingsHint = state.statusMessage && state.statusMessage.toLowerCase().includes("settings");
        cardsContainer.innerHTML = \`<div class="empty-state">
          <svg class="svg-icon" style="width: 24px; height: 24px; opacity: 0.3;" viewBox="0 0 16 16"><path d="M8 15A7 7 0 1 1 8 1a7 7 0 0 1 0 14zm0 1A8 8 0 1 0 8 0a8 8 0 0 0 0 16z"></path></svg>
          <span>\${escapeHtml(state.statusMessage) || "No memories found for this view."}</span>
          \${hasSettingsHint ? '<button class="feedback-submit-btn" data-action="open-settings-hint" style="margin-top: 8px; font-size: 11px; padding: 4px 12px;">Open Settings</button>' : ""}
        </div>\`;
        return;
      }

      cardsContainer.innerHTML = "";
      filtered.forEach(memory => {
        const card = document.createElement("div");
        card.className = "card";
        
        // Escape memory ID for safe HTML attribute interpolation
        const safeId = escapeAttr(memory.id || "");

        // Header
        const isPinned = !!memory.pinned;
        const typeClass = memory.memory_type === "semantic" ? "badge-semantic" : "badge-episodic";
        const dateStr = memory.updated_at ? relativeDate(memory.updated_at) : "";
        const isWorkspace = memory.scope_visibility === "workspace";

        // Badges
        let badgeHtml = \`<span class="badge \${typeClass}">\${memory.memory_type || "episodic"}</span>\`;
        if (isWorkspace) {
          badgeHtml += \` <span class="badge badge-workspace">workspace</span>\`;
        }

        // Tags
        let tagsHtml = "";
        if (Array.isArray(memory.tags) && memory.tags.length > 0) {
          tagsHtml = \`<div class="tags-container">\` + 
            memory.tags.map(t => \`<span class="tag">\${escapeHtml(t)}</span>\`).join("") + 
            \`</div>\`;
        }

        // Meters
        let meterHtml = "";
        if (typeof memory.importance_score === "number") {
          const impPercent = Math.round(memory.importance_score * 100);
          meterHtml += \`
            <div class="meter-row">
              <span class="meter-label">Importance</span>
              <div class="meter-bar-container">
                <div class="meter-bar meter-bar-importance" style="width: \${impPercent}%"></div>
              </div>
              <span class="meter-val">\${memory.importance_score.toFixed(2)}</span>
            </div>
          \`;
        }
        if (typeof memory.score === "number") {
          // Normalize score to percent assuming max score around 1.0 (or just cap)
          const relPercent = Math.min(100, Math.round(memory.score * 100));
          meterHtml += \`
            <div class="meter-row">
              <span class="meter-label">Relevance</span>
              <div class="meter-bar-container">
                <div class="meter-bar meter-bar-relevance" style="width: \${relPercent}%"></div>
              </div>
              <span class="meter-val">\${memory.score.toFixed(2)}</span>
            </div>
          \`;
        }

        if (meterHtml) {
          meterHtml = \`<div class="meters">\${meterHtml}</div>\`;
        }

        // Collapse checks
        const contentText = memory.content || "";
        const needsTruncate = contentText.length > 180;
        const displayContent = needsTruncate ? contentText : contentText;

        // Action Toolbar
        const isEpisodic = memory.memory_type === "episodic";
        const isSemantic = memory.memory_type === "semantic";
        
        let promoteBtn = "";
        if (isEpisodic) {
          promoteBtn = \`
            <button class="action-btn action-btn-accent" data-action="promote" data-id="\${safeId}" title="Promote to Semantic">
              <svg class="svg-icon" viewBox="0 0 16 16"><path d="M8 0L3 5h3v6h4V5h3L8 0zm-5 13h10v2H3v-2z"/></svg>
            </button>
          \`;
        }

        let publishBtn = "";
        if (isSemantic && !isWorkspace) {
          publishBtn = \`
            <button class="action-btn action-btn-accent" data-action="publish" data-id="\${safeId}" title="Publish to Workspace Pool">
              <svg class="svg-icon" viewBox="0 0 16 16"><path d="M8 0a8 8 0 1 0 0 16A8 8 0 0 0 8 0zM7 11.5H5.5a2 2 0 0 1 0-4H7v4zm3.5-4h-2v4h2a2 2 0 0 0 0-4z"/></svg>
            </button>
          \`;
        }

        // Feedback Buttons (only visible if we have a queryId)
        let feedbackHtml = "";
        const queryId = memory.query_id || memory.queryId;
        if (queryId) {
          feedbackHtml = \`
            <div class="feedback-trigger-row" data-no-card-open>
              <button class="feedback-trigger-btn thumbs-up" data-action="feedback-up" data-id="\${safeId}" title="Helpful (+1)">
                <svg class="svg-icon" viewBox="0 0 16 16"><path d="M11 5.08V2c0-1.1-.9-2-2-2H8c-.55 0-1 .45-1 1v2.58l-3.3 3.3a1.98 1.98 0 0 0-.58 1.41V14c0 1.1.9 2 2 2h6c.83 0 1.54-.5 1.84-1.22l2-4.67c.1-.26.16-.54.16-.83v-3.2a2.006 2.006 0 0 0-2-2h-3.16zM0 8h2v8H0V8z"/></svg>
              </button>
              <button class="feedback-trigger-btn thumbs-down" data-action="feedback-down" data-id="\${safeId}" title="Not Helpful (-1)">
                <svg class="svg-icon" viewBox="0 0 16 16"><path d="M5 10.92V14c0 1.1.9 2 2 2h1c.55 0 1-.45 1-1v-2.58l3.3-3.3c.37-.37.58-.88.58-1.41V2c0-1.1-.9-2-2-2H5c-.83 0-1.54.5-1.84 1.22l-2 4.67c-.1.26-.16.54-.16.83v3.2c0 1.1.9 2 2 2h3.16zM16 8h-2v-8h2v8z"/></svg>
              </button>
            </div>
            <div class="feedback-comment-box" id="feedback-box-\${safeId}" data-no-card-open>
              <textarea class="feedback-comment-input" id="feedback-comment-\${safeId}" placeholder="Explain your rating (optional)..." rows="2"></textarea>
              <div class="feedback-submit-row">
                <button class="feedback-cancel-btn" data-action="feedback-cancel" data-id="\${safeId}">Cancel</button>
                <button class="feedback-submit-btn" data-action="feedback-submit" data-id="\${safeId}" id="feedback-submit-btn-\${safeId}">Submit</button>
              </div>
            </div>
          \`;
        }

        card.innerHTML = \`
          <div class="card-header">
            <div class="card-badges">
              \${badgeHtml}
            </div>
            <div class="card-header-right">
              <span class="card-date">\${dateStr}</span>
              <button class="pin-btn \${isPinned ? "pinned" : ""}" data-action="pin" data-id="\${safeId}" data-pinned="\${!isPinned}" title="\${isPinned ? "Unpin Memory" : "Pin Memory"}">
                <svg class="svg-icon" viewBox="0 0 16 16"><path d="M12.9 8.2v-6h1.1v-1h-12v1h1.1v6l-2.1 2.1v1h5.3v3.7l1.1 1.1 1.1-1.1v-3.7h5.3v-1l-1.9-2.1z"/></svg>
              </button>
            </div>
          </div>
          <div class="card-content" id="content-\${safeId}">
            \${escapeHtml(displayContent)}
            \${needsTruncate ? '<div class="content-fade"></div>' : ""}
          </div>
          \${needsTruncate ? \`<button class="read-more-btn" id="read-more-btn-\${safeId}" data-action="read-more" data-id="\${safeId}">Read more</button>\` : ""}
          \${tagsHtml}
          \${meterHtml}
          <div class="card-footer">
            <div class="footer-left">
              \${feedbackHtml}
            </div>
            <div class="footer-right">
              <button class="action-btn" data-action="copy" data-id="\${safeId}" title="Copy Content">
                <svg class="svg-icon" viewBox="0 0 16 16"><path d="M4 4h8v1H4V4zm0 2h8v1H4V6zm0 2h8v1H4V8zm-2-6h12v12H2V2zm1 1v10h10V3H3z"/></svg>
              </button>
              <button class="action-btn" data-action="edit" data-id="\${safeId}" title="Edit Memory">
                <svg class="svg-icon" viewBox="0 0 16 16"><path d="M12.146.146a.5.5 0 0 1 .708 0l3 3a.5.5 0 0 1 0 .708l-10 10a.5.5 0 0 1-.168.11l-5 2a.5.5 0 0 1-.65-.65l2-5a.5.5 0 0 1 .11-.168l10-10zM11.207 2.5L13.5 4.793 14.793 3.5 12.5 1.207 11.207 2.5zm1.586 3L10.5 3.207 4 9.707V12h2.293l6.5-6.5z"/></svg>
              </button>
              \${promoteBtn}
              \${publishBtn}
              <button class="action-btn action-btn-danger" data-action="delete" data-id="\${safeId}" title="Delete Memory">
                <svg class="svg-icon" viewBox="0 0 16 16"><path d="M5.5 5.5A.5.5 0 0 1 6 6v6a.5.5 0 0 1-1 0V6c0-.28.22-.5.5-.5zm2.5 0a.5.5 0 0 1 .5.5v6a.5.5 0 0 1-1 0V6c0-.28.22-.5.5-.5zm3-.5a.5.5 0 0 0-.5.5v6a.5.5 0 0 0 1 0V6c0-.28-.22-.5-.5-.5zM11 2.5V1h-6v1.5H2.5A.5.5 0 0 0 2 3v1h12V3a.5.5 0 0 0-.5-.5H11zM13 5H3v10a1 1 0 0 0 1 1h8a1 1 0 0 0 1-1V5z"/></svg>
              </button>
            </div>
          </div>
        \`;

        // The id lives on the element so the delegated handler can open details.
        card.dataset.id = memory.id || "";

        cardsContainer.appendChild(card);
      });
    }

    // Helper functions — invoked from the delegated click handler above.
    function toggleReadMore(id) {
      const contentEl = document.getElementById(\`content-\${id}\`);
      const btnEl = document.getElementById(\`read-more-btn-\${id}\`);
      if (contentEl.classList.contains("expanded")) {
        contentEl.classList.remove("expanded");
        btnEl.textContent = "Read more";
      } else {
        contentEl.classList.add("expanded");
        btnEl.textContent = "Read less";
      }
    }

    // Feedback loops inside Webview
    let activeFeedbackRating = {}; // maps memoryId -> rating

    function toggleFeedbackPanel(id, rating) {
      const box = document.getElementById(\`feedback-box-\${id}\`);
      const ups = box.previousElementSibling.querySelectorAll(".feedback-trigger-btn");

      activeFeedbackRating[id] = rating;

      // Toggle styles
      if (rating === 1) {
        ups[0].classList.add("active");
        ups[1].classList.remove("active");
      } else {
        ups[0].classList.remove("active");
        ups[1].classList.add("active");
      }

      box.style.display = "flex";
    }

    function submitFeedback(id) {
      const comment = document.getElementById(\`feedback-comment-\${id}\`).value;
      const memory = state.memories.find(m => m.id === id);
      const queryId = memory && (memory.query_id || memory.queryId);
      vscode.postMessage({
        type: "submitFeedback",
        id,
        queryId,
        rating: activeFeedbackRating[id],
        comment: comment.trim() || null
      });
      closeFeedbackPanel(id);
    }

    function closeFeedbackPanel(id) {
      const box = document.getElementById(\`feedback-box-\${id}\`);
      box.style.display = "none";
      const ups = box.previousElementSibling.querySelectorAll(".feedback-trigger-btn");
      ups[0].classList.remove("active");
      ups[1].classList.remove("active");
      document.getElementById(\`feedback-comment-\${id}\`).value = "";
    };

    // Utility text escaping
    function escapeHtml(text) {
      const map = {
        '&': '&amp;',
        '<': '&lt;',
        '>': '&gt;',
        '"': '&quot;',
        "'": '&#039;'
      };
      return text.replace(/[&<>"']/g, function(m) { return map[m]; });
    }

    // Escape for safe interpolation into HTML attributes (onclick handlers, id, etc.)
    function escapeAttr(text) {
      return escapeHtml(String(text)).replace(/\\/g, '\\\\');
    }

    function unescapeHtml(text) {
      const map = {
        '&amp;': '&',
        '&lt;': '<',
        '&gt;': '>',
        '&quot;': '"',
        '&#039;': "'"
      };
      return text.replace(/&amp;|&lt;|&gt;|&quot;|&#039;/g, function(m) { return map[m]; });
    }

    // Relative Date helper
    function relativeDate(value) {
      const timestamp = Date.parse(value);
      if (isNaN(timestamp)) {
        return "";
      }

      const elapsedMs = Date.now() - timestamp;
      const elapsedMinutes = Math.round(elapsedMs / 60000);
      if (elapsedMinutes < 1) {
        return "just now";
      }
      if (elapsedMinutes < 60) {
        return \`\${elapsedMinutes}m ago\`;
      }

      const elapsedHours = Math.round(elapsedMinutes / 60);
      if (elapsedHours < 48) {
        return \`\${elapsedHours}h ago\`;
      }

      const elapsedDays = Math.round(elapsedHours / 24);
      return \`\${elapsedDays}d ago\`;
    }
  </script>
</body>
</html>
`;
  }
}

// Random nonce so the inline webview <script> is permitted under the CSP.
function getNonce(): string {
  let text = "";
  const possible = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
  for (let i = 0; i < 32; i++) {
    text += possible.charAt(Math.floor(Math.random() * possible.length));
  }
  return text;
}
