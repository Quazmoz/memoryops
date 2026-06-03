# MemoryOps for VS Code

**Governed, structured agent memory — right inside your editor.**

MemoryOps gives AI-assisted development workflows a durable memory layer. Browse, search, curate, and retrieve workspace memories without leaving VS Code. Connect to your MemoryOps backend and let your agents remember what matters.

---

## Features

### 🧠 Memory Sidebar

A rich webview sidebar lets you browse all workspace memories at a glance. Filter by memory type (episodic / semantic), pinned status, and sort by importance, relevance, or recency. Pagination keeps things fast even with large memory stores.

### 🔍 Semantic & Hybrid Search

Search your entire MemoryOps workspace using hybrid, keyword, or vector search modes directly from the Command Palette or the sidebar search bar. Results are ranked by relevance with live inline search.

### 📎 Context Retrieval

Retrieve curated context for the file or selection you're working on. MemoryOps packs the most relevant memories into a token-budgeted context block you can insert at the cursor or copy to clipboard — ideal for feeding context into AI chat workflows.

### ✏️ Full Memory CRUD

Create observations from selected code or notes, edit memory content in a full editor buffer, update tags and importance scores, promote episodic memories to semantic, publish to the workspace pool, pin/unpin, merge, and delete — all from the sidebar or Command Palette.

### 📊 History, Provenance & Feedback

Inspect version history, trace provenance graphs, view retrieval feedback, and submit your own relevance ratings to improve future recall quality.

### ⚡ Bulk Operations

Select multiple memories and pin, unpin, or delete them in one action.

### 💬 Copilot Chat Participant

Type **`@memoryops`** in GitHub Copilot Chat to query your workspace conversationally. Use `/search` (default) for matching memories or `/retrieve` for packed, token-budgeted context — complete with "Open in editor" buttons and follow-up suggestions.

### 📌 Inline CodeLens Hints

Opt in with `memoryops.enableCodeLens` to see how many memories reference the file you're editing, right at the top of the editor. Click the lens to surface them.

### 🔄 Resilient Connectivity

Read-only requests automatically retry on transient backend hiccups with exponential backoff. When a connection fails you get one-click **Reconnect** from both the notification and the status bar.

### 🚀 Guided Onboarding

A built-in Getting Started walkthrough opens on first install and walks you through connecting, authenticating, and verifying — re-openable any time via `MemoryOps: Open Getting Started Walkthrough`.

---

## Getting Started

1. **Install the extension** from the VS Code Marketplace.
2. **Open Settings** (`Ctrl+,` / `Cmd+,`) and search for `MemoryOps`.
3. **Configure** your API URL, Workspace ID, and API Key.
4. **Click the MemoryOps icon** in the Activity Bar to open the sidebar.
5. **Test the connection** using the `MemoryOps: Test Connection` command from the Command Palette.

> **Tip:** Store your API key in **User Settings** (not workspace settings) to avoid committing secrets.

---

## Requirements

- A running [MemoryOps](https://github.com/Quazmoz/memoryops) backend (self-hosted or cloud).
- A valid MemoryOps workspace UUID and API key.

---

## Settings

| Setting | Default | Description |
|---|---|---|
| `memoryops.apiUrl` | `http://localhost:8080` | Base URL for the MemoryOps API |
| `memoryops.workspaceId` | — | MemoryOps workspace UUID |
| `memoryops.apiKey` | — | Workspace API key (store in user settings) |
| `memoryops.defaultTopK` | `5` | Number of results for search/retrieval (1–20) |
| `memoryops.defaultSearchMode` | `hybrid` | Search mode: `hybrid`, `keyword`, or `vector` |
| `memoryops.defaultTokenBudget` | `2048` | Token budget for retrieval context (256–16000) |
| `memoryops.sidebarPageSize` | `20` | Memories per page in the sidebar (1–100) |
| `memoryops.includeWorkspacePool` | `false` | Include workspace-published memories in search |
| `memoryops.defaultAgentId` | `vscode` | Agent identifier for observations saved from VS Code |
| `memoryops.maxRetries` | `3` | Auto-retries for read-only requests on transient failures (0–10; `0` disables) |
| `memoryops.retryBackoffMs` | `500` | Base delay (ms) for exponential backoff between retries (0–10000) |
| `memoryops.enableCodeLens` | `false` | Show an inline CodeLens with the count of memories referencing the current file |

---

## Commands

All commands are available from the Command Palette (`Ctrl+Shift+P` / `Cmd+Shift+P`):

| Command | Description |
|---|---|
| `MemoryOps: Test Connection` | Verify API connectivity and workspace access |
| `MemoryOps: Refresh Memories` | Reload recent memories in the sidebar |
| `MemoryOps: Search Memory` | Search workspace memories |
| `MemoryOps: Retrieve Context for Current File` | Get relevant context for the active file/selection |
| `MemoryOps: Save Selection as Observation` | Save selected text as a new memory observation |
| `MemoryOps: Open Memory` | View full memory details in a markdown document |
| `MemoryOps: Edit Memory` | Edit content, tags, or importance score |
| `MemoryOps: Promote Memory` | Promote an episodic memory to semantic |
| `MemoryOps: Publish Memory To Workspace` | Publish a semantic memory to the workspace pool |
| `MemoryOps: Merge Memory` | Merge two semantic memories |
| `MemoryOps: View Memory History` | View version history |
| `MemoryOps: View Memory Provenance` | View the provenance graph |
| `MemoryOps: View Memory Feedback` | View retrieval feedback entries |
| `MemoryOps: Submit Retrieval Feedback` | Rate a retrieved memory's relevance |
| `MemoryOps: Insert Retrieval Context` | Insert retrieved context at the cursor |
| `MemoryOps: Copy Retrieval Context` | Copy retrieved context to clipboard |
| `MemoryOps: Filter and Sort Sidebar` | Change sidebar filters and sort order |
| `MemoryOps: Bulk Operations` | Pin, unpin, or delete multiple memories at once |
| `MemoryOps: Pin / Unpin Memory` | Toggle memory pinning |
| `MemoryOps: Delete Memory` | Delete a memory |
| `MemoryOps: Copy Memory Content` | Copy memory content to clipboard |
| `MemoryOps: Show Memories Referencing Current File` | Find memories related to the active file |
| `MemoryOps: Reconnect` | Re-establish the backend connection |
| `MemoryOps: Open Getting Started Walkthrough` | Reopen the onboarding walkthrough |
| `MemoryOps: Open Settings` | Open extension settings |

---

## Contributing

Contributions, issues, and feature requests are welcome! See the [contributing guide](https://github.com/Quazmoz/memoryops/blob/main/CONTRIBUTING.md) and [repository](https://github.com/Quazmoz/memoryops).

## License

[MIT](LICENSE) © Quazmoz
