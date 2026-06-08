# MemoryOps VS Code Extension

MemoryOps includes an early VS Code extension scaffold under:

```text
extensions/vscode-memoryops
```

The extension is not published to the Visual Studio Marketplace yet. It is intended for local development, dogfooding, and iteration before Marketplace packaging.

## Current Features

The extension currently includes these commands and sidebar actions:

| Command | Purpose |
|--------|---------|
| `MemoryOps: Test Connection` | Checks API readiness and workspace access. |
| `MemoryOps: Refresh Memories` | Loads recent workspace memories into the MemoryOps sidebar. |
| Sidebar auto-load and paging | Automatically hydrates recent memories when settings are ready; supports paging with a **Load More** button. |
| `MemoryOps: Search Memory` | Searches the configured MemoryOps workspace from the Command Palette. |
| `MemoryOps: Retrieve Context for Current File` | Sends the active file/selection as retrieval context and opens returned memory context in a Markdown preview document. |
| `MemoryOps: Save Selection as Observation` | Sends selected code or notes to `/v1/ingest/observation`. |
| `MemoryOps: Promote Memory` | Promotes an episodic memory to semantic memory. |
| `MemoryOps: Publish Memory To Workspace` | Publishes a semantic memory to the workspace pool. |
| `MemoryOps: View Memory History` | Opens version history for a selected memory. |
| `MemoryOps: View Memory Provenance` | Opens the provenance graph for a selected memory. |
| `MemoryOps: View Memory Feedback` | Shows retrieval feedback recorded for a selected memory. |
| Sidebar memory actions | Open, pin, unpin, delete, and copy memory content. |
| Bulk operations | Bulk select memories using checkboxes in the sidebar; trigger bulk pin, unpin, or delete from the floating toolbar. Delete triggers a VS Code confirmation modal and performs a soft-delete alongside Qdrant vector index cleanup best-effort server-side. |
| `MemoryOps: Open Settings` | Opens MemoryOps extension settings. |

## Settings

Configure the extension in VS Code user settings:

```json
{
  "memoryops.apiUrl": "http://localhost:8080",
  "memoryops.workspaceId": "<workspace-uuid>",
  "memoryops.apiKey": "<mops-api-key>",
  "memoryops.defaultTopK": 5,
  "memoryops.defaultSearchMode": "hybrid",
  "memoryops.defaultTokenBudget": 2048,
  "memoryops.sidebarPageSize": 20,
  "memoryops.includeWorkspacePool": false,
  "memoryops.defaultAgentId": "vscode"
}
```

Store `memoryops.apiKey` in user settings. Do not commit API keys to repository-level `.vscode/settings.json`.

## Local development

```bash
cd extensions/vscode-memoryops
npm install
npm run compile
npm test
```

Then open `extensions/vscode-memoryops` in VS Code and press `F5` to launch an Extension Development Host.

## Package locally

```bash
cd extensions/vscode-memoryops
npm install
npm run package
```

This produces a `.vsix` file for local testing. Marketplace publishing is intentionally out of scope for the initial scaffold.

## Roadmap

- Optional VS Code chat participant integration.
- Extension Host tests for command wiring and end-to-end sidebar flows.
- Marketplace packaging metadata, icon, screenshots, and release workflow.
