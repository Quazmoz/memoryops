import * as vscode from "vscode";
import { ContradictionItem } from "./client";

export class ContradictionsWebviewViewProvider implements vscode.WebviewViewProvider {
  public static readonly viewType = "memoryops.contradictions";

  private _view?: vscode.WebviewView;
  private _contradictions: ContradictionItem[] = [];
  private _activeTab: string = "open";
  private _statusMessage = "Loading contradictions...";
  private _nextCursor: string | null = null;
  private _selectedIds: Set<string> = new Set();
  private _resolveTarget: string | null = null;
  private _notes = "";

  constructor(private readonly _extensionUri: vscode.Uri) {}

  public getActiveTab(): string {
    return this._activeTab;
  }

  public getNextCursor(): string | null {
    return this._nextCursor;
  }

  public getSelectedIds(): string[] {
    return Array.from(this._selectedIds);
  }

  public clearSelection(): void {
    this._selectedIds.clear();
    this.updateWebview();
  }

  public setContradictions(
    response: { items: ContradictionItem[]; next_cursor: string | null },
    options: { append?: boolean } = {}
  ): void {
    this._contradictions = options.append
      ? [...this._contradictions, ...response.items]
      : response.items;
    this._nextCursor = response.next_cursor;
    
    if (this._contradictions.length === 0) {
      this._statusMessage = `No ${this._activeTab} contradictions found.`;
    } else {
      this._statusMessage = `Showing ${this._contradictions.length} contradictions.`;
    }
    
    this.updateWebview();
  }

  public setError(message: string): void {
    this._contradictions = [];
    this._nextCursor = null;
    this._statusMessage = `Error: ${message}`;
    this.updateWebview();
  }

  public removeContradiction(flagId: string): void {
    this._contradictions = this._contradictions.filter((c) => c.id !== flagId);
    if (this._resolveTarget === flagId) {
      this._resolveTarget = null;
      this._notes = "";
    }
    this._selectedIds.delete(flagId);
    this.updateWebview();
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
        case "refresh": {
          this.refreshList();
          break;
        }
        case "tabChanged": {
          this._activeTab = data.tab;
          this._contradictions = [];
          this._nextCursor = null;
          this._selectedIds.clear();
          this._resolveTarget = null;
          this._notes = "";
          this._statusMessage = "Loading contradictions...";
          this.updateWebview();
          this.refreshList();
          break;
        }
        case "resolveClick": {
          this._resolveTarget = this._resolveTarget === data.id ? null : data.id;
          this._notes = "";
          this.updateWebview();
          break;
        }
        case "resolveSubmit": {
          vscode.commands.executeCommand("memoryops.resolveContradiction", {
            id: data.id,
            resolution: data.resolution,
            notes: data.notes,
          });
          break;
        }
        case "bulkDismiss": {
          vscode.commands.executeCommand("memoryops.bulkDismissContradictions", {
            ids: Array.from(this._selectedIds),
          });
          break;
        }
        case "selectToggle": {
          const id = data.id;
          if (this._selectedIds.has(id)) {
            this._selectedIds.delete(id);
          } else {
            this._selectedIds.add(id);
          }
          this.updateWebview();
          break;
        }
        case "selectAll": {
          const visibleOpen = this._contradictions.filter(c => c.resolution === "open");
          if (this._selectedIds.size === visibleOpen.length) {
            this._selectedIds.clear();
          } else {
            this._selectedIds = new Set(visibleOpen.map(c => c.id));
          }
          this.updateWebview();
          break;
        }
        case "openDetails": {
          // Open Memory A or B in full view
          vscode.commands.executeCommand("memoryops.openMemory", { id: data.memoryId });
          break;
        }
        case "loadMore": {
          vscode.commands.executeCommand("memoryops.refreshContradictions", { append: true });
          break;
        }
      }
    });

    this.updateWebview();
  }

  public refreshList(): void {
    vscode.commands.executeCommand("memoryops.refreshContradictions");
  }

  public updateWebview(): void {
    if (!this._view) {
      return;
    }
    const hasMore = !!this._nextCursor;
    this._view.webview.postMessage({
      type: "state",
      contradictions: this._contradictions,
      activeTab: this._activeTab,
      statusMessage: this._statusMessage,
      selectedIds: Array.from(this._selectedIds),
      resolveTarget: this._resolveTarget,
      hasMore,
    });
  }

  private _getHtmlForWebview(webview: vscode.Webview): string {
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
  <title>MemoryOps Contradictions</title>
  <style>
    :root {
      --accent-purple: #8b5cf6;
      --accent-blue: #3b82f6;
      --accent-pink: #ec4899;
      --accent-amber: #f59e0b;
      --accent-red: #ef4444;
      --accent-green: #10b981;
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
      justify-content: space-between;
      gap: 8px;
    }

    .title {
      font-size: 13px;
      font-weight: 600;
      text-transform: uppercase;
      letter-spacing: 0.5px;
      opacity: 0.8;
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
      overflow-x: auto;
    }

    .tab {
      padding: 5px 8px;
      border-radius: 4px;
      cursor: pointer;
      transition: all 0.2s ease;
      font-weight: 500;
      opacity: 0.6;
      border: none;
      background: transparent;
      color: var(--vscode-editor-foreground);
      font-size: 11px;
      white-space: nowrap;
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

    .bulk-toolbar {
      display: none;
      align-items: center;
      justify-content: space-between;
      background: rgba(245, 158, 11, 0.1);
      border: 1px solid var(--accent-amber);
      border-radius: 6px;
      padding: 6px 8px;
      margin-top: 4px;
      box-sizing: border-box;
    }

    .bulk-selected-count {
      font-weight: 600;
      color: var(--vscode-editor-foreground);
    }

    .bulk-actions {
      display: flex;
      gap: 4px;
    }

    .bulk-btn {
      background: var(--vscode-button-background);
      color: var(--vscode-button-foreground);
      border: none;
      padding: 3px 6px;
      border-radius: 3px;
      font-size: 10px;
      font-weight: 500;
      cursor: pointer;
    }

    .bulk-btn:hover {
      background: var(--vscode-button-hoverBackground);
    }

    .bulk-btn-secondary {
      background: var(--vscode-button-secondaryBackground, rgba(255, 255, 255, 0.1));
      color: var(--vscode-button-secondaryForeground, var(--vscode-editor-foreground));
      border: 1px solid var(--vscode-button-border, rgba(255, 255, 255, 0.15));
    }

    .bulk-btn-secondary:hover {
      background: var(--vscode-button-secondaryHoverBackground, rgba(255, 255, 255, 0.15));
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

    .badge-conflict {
      background: rgba(239, 68, 68, 0.15);
      color: var(--accent-red);
      border: 1px solid rgba(239, 68, 68, 0.25);
    }

    .badge-similarity {
      background: rgba(59, 130, 246, 0.15);
      color: var(--accent-blue);
      border: 1px solid rgba(59, 130, 246, 0.25);
    }

    .badge-resolution {
      background: rgba(16, 185, 129, 0.15);
      color: var(--accent-green);
      border: 1px solid rgba(16, 185, 129, 0.25);
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

    .card-checkbox {
      cursor: pointer;
      width: 14px;
      height: 14px;
      margin: 0;
      accent-color: var(--accent-purple);
    }

    /* Pair Container */
    .pair-container {
      display: flex;
      flex-direction: column;
      gap: 8px;
      margin-top: 4px;
    }

    .memory-box {
      border: 1px solid var(--card-border);
      border-radius: 6px;
      background: rgba(0, 0, 0, 0.15);
      padding: 8px;
      transition: all 0.2s ease;
      cursor: pointer;
    }

    .memory-box:hover {
      border-color: var(--accent-purple);
      background: rgba(139, 92, 246, 0.05);
    }

    .memory-title-row {
      display: flex;
      justify-content: space-between;
      font-size: 9px;
      font-weight: 600;
      opacity: 0.5;
      text-transform: uppercase;
      margin-bottom: 4px;
    }

    .memory-content {
      font-size: 11px;
      line-height: 1.4;
      white-space: pre-wrap;
      color: var(--vscode-editor-foreground);
    }

    /* Winner/Loser tags */
    .winner-tag {
      background: rgba(16, 185, 129, 0.15);
      color: var(--accent-green);
      padding: 1px 4px;
      border-radius: 3px;
      font-size: 9px;
      font-weight: 600;
      border: 1px solid rgba(16, 185, 129, 0.25);
    }

    .archived-tag {
      background: rgba(239, 68, 68, 0.15);
      color: var(--accent-red);
      padding: 1px 4px;
      border-radius: 3px;
      font-size: 9px;
      font-weight: 600;
      border: 1px solid rgba(239, 68, 68, 0.25);
      text-decoration: line-through;
    }

    /* Resolve Action Dropdown */
    .resolve-trigger-btn {
      background: var(--vscode-button-background);
      color: var(--vscode-button-foreground);
      border: none;
      padding: 4px 10px;
      border-radius: 4px;
      cursor: pointer;
      font-size: 11px;
      font-weight: 500;
      align-self: flex-start;
      margin-top: 4px;
    }

    .resolve-trigger-btn:hover {
      background: var(--vscode-button-hoverBackground);
    }

    .resolve-panel {
      display: none;
      flex-direction: column;
      gap: 6px;
      background: rgba(0, 0, 0, 0.25);
      border: 1px solid var(--card-border);
      border-radius: 6px;
      padding: 8px;
      margin-top: 6px;
    }

    .resolve-notes {
      background: var(--vscode-input-background);
      color: var(--vscode-input-foreground);
      border: 1px solid var(--vscode-input-border, transparent);
      padding: 4px 6px;
      border-radius: 4px;
      font-size: 11px;
      outline: none;
      width: 100%;
      box-sizing: border-box;
      resize: vertical;
    }

    .resolve-notes:focus {
      border-color: var(--vscode-focusBorder);
    }

    .resolve-actions-grid {
      display: grid;
      grid-template-columns: 1fr 1fr;
      gap: 4px;
    }

    .resolve-btn {
      border: none;
      padding: 4px;
      border-radius: 3px;
      font-size: 10px;
      font-weight: 500;
      cursor: pointer;
      text-align: center;
      color: #fff;
    }

    .resolve-btn-keep-a, .resolve-btn-keep-b {
      background: #047857;
    }
    .resolve-btn-keep-a:hover, .resolve-btn-keep-b:hover {
      background: #065f46;
    }
    .resolve-btn-accept {
      background: #1e3a8a;
    }
    .resolve-btn-accept:hover {
      background: #172554;
    }
    .resolve-btn-dismiss {
      background: #374151;
    }
    .resolve-btn-dismiss:hover {
      background: #1f2937;
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
      <span class="title">Contradictions</span>
      <button class="icon-button" id="refresh-btn" title="Refresh Contradictions">
        <svg class="svg-icon" viewBox="0 0 16 16"><path d="M13.6 2.3C12.2.9 10.2.1 8 .1 3.6.1 0 3.7 0 8.1s3.6 8 8 8c3.2 0 6-1.9 7.2-4.8l-1.3-.5c-1 2.3-3.2 3.8-5.9 3.8-3.6 0-6.5-2.9-6.5-6.5S4.4 1.6 8 1.6c1.8 0 3.4.7 4.6 1.9l-2.1 2.1h5.6V0L13.6 2.3z"/></svg>
      </button>
    </div>
    <div class="tabs">
      <button class="tab active" data-tab="open">Open</button>
      <button class="tab" data-tab="auto_resolved">Auto-resolved</button>
      <button class="tab" data-tab="keep_a">Keep A</button>
      <button class="tab" data-tab="keep_b">Keep B</button>
      <button class="tab" data-tab="dismissed">Dismissed</button>
      <button class="tab" data-tab="accepted">Accepted</button>
    </div>
    <div class="status-bar" id="status">Loading...</div>
    <div class="bulk-toolbar" id="bulk-toolbar">
      <span class="bulk-selected-count" id="bulk-selected-count">0 selected</span>
      <div class="bulk-actions">
        <button class="bulk-btn" id="bulk-dismiss-btn" title="Dismiss selected">Dismiss Flags</button>
        <button class="bulk-btn bulk-btn-secondary" id="bulk-clear-btn" title="Clear selection">Clear</button>
      </div>
    </div>
  </div>

  <div class="cards-list" id="cards-container">
    <div class="empty-state">
      <svg class="svg-icon" style="width: 32px; height: 32px; opacity: 0.3;" viewBox="0 0 16 16"><path d="M8 0a8 8 0 1 0 0 16A8 8 0 0 0 8 0zm1 12H7v-2h2v2zm0-3H7V4h2v5z"/></svg>
      <span>No contradictions loaded.</span>
    </div>
  </div>

  <div id="load-more-container" style="display: none; justify-content: center; padding: 10px 0 20px 0;">
    <button class="bulk-btn bulk-btn-secondary" id="load-more-btn" style="width: 100%; padding: 8px;">Load More</button>
  </div>

  <script nonce="${nonce}">
    const vscode = acquireVsCodeApi();
    window.vscode = vscode;

    // Report errors back to extension host
    window.onerror = function(message, source, lineno, colno, error) {
      vscode.postMessage({
        type: "error",
        message: message,
        source: source,
        lineno: lineno,
        colno: colno,
        error: error ? error.stack : undefined
      });
      return false;
    };

    // DOM Cache
    const cardsContainer = document.getElementById("cards-container");
    const refreshBtn = document.getElementById("refresh-btn");
    const statusBar = document.getElementById("status");
    const tabs = document.querySelectorAll(".tab");
    const bulkToolbar = document.getElementById("bulk-toolbar");
    const bulkSelectedCount = document.getElementById("bulk-selected-count");
    const bulkDismissBtn = document.getElementById("bulk-dismiss-btn");
    const bulkClearBtn = document.getElementById("bulk-clear-btn");
    const loadMoreContainer = document.getElementById("load-more-container");
    const loadMoreBtn = document.getElementById("load-more-btn");

    // Local State
    let state = {
      contradictions: [],
      activeTab: "open",
      statusMessage: "Loading...",
      selectedIds: [],
      resolveTarget: null,
      hasMore: false
    };

    refreshBtn.addEventListener("click", () => vscode.postMessage({ type: "refresh" }));

    window.addEventListener("message", event => {
      const message = event.data;
      if (message.type === "state") {
        state.contradictions = message.contradictions || [];
        state.activeTab = message.activeTab || "open";
        state.statusMessage = message.statusMessage || "";
        state.selectedIds = message.selectedIds || [];
        state.resolveTarget = message.resolveTarget;
        state.hasMore = !!message.hasMore;

        // Sync tabs active state
        tabs.forEach(tab => {
          if (tab.getAttribute("data-tab") === state.activeTab) {
            tab.classList.add("active");
          } else {
            tab.classList.remove("active");
          }
        });

        // Sync status bar
        statusBar.textContent = state.statusMessage;

        // Render contradictions list
        renderContradictions();
      }
    });

    // Tab switching
    tabs.forEach(tab => {
      tab.addEventListener("click", () => {
        const selectedTab = tab.getAttribute("data-tab");
        vscode.postMessage({ type: "tabChanged", tab: selectedTab });
      });
    });

    // Bulk action click listeners
    bulkDismissBtn.addEventListener("click", () => {
      vscode.postMessage({ type: "bulkDismiss" });
    });

    bulkClearBtn.addEventListener("click", () => {
      vscode.postMessage({ type: "selectAll" }); // Deselect all
    });

    loadMoreBtn.addEventListener("click", () => {
      vscode.postMessage({ type: "loadMore" });
    });

    // Delegated click handling
    cardsContainer.addEventListener("click", (e) => {
      // Check for checkbox click
      const cb = e.target.closest(".card-checkbox");
      if (cb) {
        vscode.postMessage({ type: "selectToggle", id: cb.dataset.id });
        return;
      }

      // Check for resolve toggle click
      const resolveToggleBtn = e.target.closest("[data-action='resolve-toggle']");
      if (resolveToggleBtn) {
        vscode.postMessage({ type: "resolveClick", id: resolveToggleBtn.dataset.id });
        return;
      }

      // Check for resolve submit buttons
      const resolveBtn = e.target.closest("[data-action='resolve-submit']");
      if (resolveBtn) {
        const id = resolveBtn.dataset.id;
        const resolution = resolveBtn.dataset.resolution;
        const notes = document.getElementById(\`notes-\${id}\`).value;
        vscode.postMessage({ type: "resolveSubmit", id, resolution, notes: notes.trim() });
        return;
      }

      // Check for memory box click (opens detail view)
      const memoryBox = e.target.closest("[data-memory-id]");
      if (memoryBox) {
        vscode.postMessage({ type: "openDetails", memoryId: memoryBox.dataset.memoryId });
        return;
      }
    });

    function updateBulkToolbar() {
      if (state.activeTab === "open" && state.selectedIds.length > 0) {
        bulkSelectedCount.textContent = \`\${state.selectedIds.length} selected\`;
        bulkToolbar.style.display = "flex";
      } else {
        bulkToolbar.style.display = "none";
      }
    }

    function renderContradictions() {
      if (state.contradictions.length === 0) {
        cardsContainer.innerHTML = \`<div class="empty-state">
          <svg class="svg-icon" style="width: 24px; height: 24px; opacity: 0.3;" viewBox="0 0 16 16"><path d="M8 15A7 7 0 1 1 8 1a7 7 0 0 1 0 14zm0 1A8 8 0 1 0 8 0a8 8 0 0 0 0 16z"></path></svg>
          <span>\${escapeHtml(state.statusMessage)}</span>
        </div>\`;
        loadMoreContainer.style.display = "none";
        updateBulkToolbar();
        return;
      }

      cardsContainer.innerHTML = "";
      state.contradictions.forEach(item => {
        const card = document.createElement("div");
        card.className = "card";
        
        const safeId = escapeAttr(item.id);
        const conflictPercent = Math.round(item.conflict_score * 100);
        const simPercent = Math.round(item.similarity * 100);
        
        const isChecked = state.selectedIds.includes(item.id);
        const isResolved = item.resolution !== "open";

        // Checkbox HTML (only if open flag)
        const checkboxHtml = state.activeTab === "open"
          ? \`<input type="checkbox" class="card-checkbox" data-id="\${safeId}" \${isChecked ? "checked" : ""} />\`
          : "";

        // Status Badge HTML
        let statusBadge = "";
        if (isResolved) {
          statusBadge = \`<span class="badge badge-resolution">\${item.resolution.replace("_", " ")}</span>\`;
        }

        // Winner/Loser labels for keep resolutions
        let winnerA = "";
        let winnerB = "";
        if (item.kept_memory_id) {
          if (item.kept_memory_id === item.memory_a.id) {
            winnerA = \`<span class="winner-tag">Winner</span>\`;
            winnerB = \`<span class="archived-tag">Archived</span>\`;
          } else {
            winnerA = \`<span class="archived-tag">Archived</span>\`;
            winnerB = \`<span class="winner-tag">Winner</span>\`;
          }
        }

        // Resolve Dropdown Panel
        let resolveBtnHtml = "";
        let resolvePanelHtml = "";
        if (state.activeTab === "open") {
          resolveBtnHtml = \`<button class="resolve-trigger-btn" data-action="resolve-toggle" data-id="\${safeId}">Resolve Flag</button>\`;
          
          const isPanelOpen = state.resolveTarget === item.id;
          resolvePanelHtml = \`
            <div class="resolve-panel" id="panel-\${safeId}" style="display: \${isPanelOpen ? "flex" : "none"};">
              <textarea class="resolve-notes" id="notes-\${safeId}" placeholder="Notes (optional)..." rows="2"></textarea>
              <div class="resolve-actions-grid">
                <button class="resolve-btn resolve-btn-keep-a" data-action="resolve-submit" data-id="\${safeId}" data-resolution="keep_a">Keep A</button>
                <button class="resolve-btn resolve-btn-keep-b" data-action="resolve-submit" data-id="\${safeId}" data-resolution="keep_b">Keep B</button>
                <button class="resolve-btn resolve-btn-accept" data-action="resolve-submit" data-id="\${safeId}" data-resolution="accepted">Accept Both</button>
                <button class="resolve-btn resolve-btn-dismiss" data-action="resolve-submit" data-id="\${safeId}" data-resolution="dismissed">Dismiss Flag</button>
              </div>
            </div>
          \`;
        }

        card.innerHTML = \`
          <div class="card-header">
            <div class="card-badges">
              \${checkboxHtml}
              <span class="badge badge-conflict">Conflict \${conflictPercent}%</span>
              <span class="badge badge-similarity">Similarity \${simPercent}%</span>
              \${statusBadge}
            </div>
            <span class="card-date">\${relativeDate(item.created_at)}</span>
          </div>

          <div class="pair-container">
            <div class="memory-box" data-memory-id="\${escapeAttr(item.memory_a.id)}" title="Click to view full memory details">
              <div class="memory-title-row">
                <span>Memory A</span>
                \${winnerA}
              </div>
              <div class="memory-content">\${escapeHtml(item.memory_a.content_preview)}</div>
            </div>
            <div class="memory-box" data-memory-id="\${escapeAttr(item.memory_b.id)}" title="Click to view full memory details">
              <div class="memory-title-row">
                <span>Memory B</span>
                \${winnerB}
              </div>
              <div class="memory-content">\${escapeHtml(item.memory_b.content_preview)}</div>
            </div>
          </div>
          
          \${resolveBtnHtml}
          \${resolvePanelHtml}
        \`;

        cardsContainer.appendChild(card);
      });

      // Load More container visibility
      if (state.hasMore) {
        loadMoreContainer.style.display = "flex";
      } else {
        loadMoreContainer.style.display = "none";
      }

      updateBulkToolbar();
    }

    function escapeHtml(text) {
      text = String(text ?? "");
      const map = {
        '&': '&amp;',
        '<': '&lt;',
        '>': '&gt;',
        '"': '&quot;',
        "'": '&#039;'
      };
      return text.replace(/[&<>"']/g, function(m) { return map[m]; });
    }

    function escapeAttr(text) {
      return escapeHtml(text);
    }

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
      if (elapsedDays < 7) {
        return \`\${elapsedDays}d ago\`;
      }

      const date = new Date(timestamp);
      return date.toLocaleDateString(undefined, {
        month: "short",
        day: "numeric",
        year: date.getFullYear() !== new Date().getFullYear() ? "numeric" : undefined
      });
    }
  </script>
</body>
</html>
`;
  }
}

function getNonce(): string {
  let text = "";
  const possible = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
  for (let i = 0; i < 32; i++) {
    text += possible.charAt(Math.floor(Math.random() * possible.length));
  }
  return text;
}
