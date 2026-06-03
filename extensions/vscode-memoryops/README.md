# MemoryOps VS Code Extension

> Early scaffold for bringing governed MemoryOps workspace memory into VS Code.

This extension is not published to the Visual Studio Marketplace yet. It is included in the MemoryOps repo for local development, dogfooding, and future packaging.

## Current Features

- `MemoryOps: Test Connection` — checks API readiness and workspace access.
- `MemoryOps: Refresh Memories` — loads recent workspace memories into the MemoryOps sidebar.
- Sidebar auto-load and pagination — automatically hydrates recent memories when settings are ready and can load additional pages from the tree.
- `MemoryOps: Search Memory` — searches workspace memory from the Command Palette.
- `MemoryOps: Retrieve Context for Current File` — requests context for the active file/selection.
- `MemoryOps: Save Selection as Observation` — sends selected code or notes to `/v1/ingest/observation`.
- `MemoryOps: Promote Memory` — promotes an episodic memory to semantic memory.
- `MemoryOps: Publish Memory To Workspace` — publishes a semantic memory to the workspace pool.
- `MemoryOps: View Memory History` — opens version history for a selected memory.
- `MemoryOps: View Memory Provenance` — opens the provenance graph for a selected memory.
- `MemoryOps: View Memory Feedback` — shows retrieval feedback recorded for a selected memory.
- Memory sidebar actions — open, pin, unpin, delete, and copy memory content.
- `MemoryOps: Open Settings` — opens extension settings.

## Settings

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

Store `memoryops.apiKey` in user settings. Do not commit API keys to workspace settings.

## Local Development

```bash
cd extensions/vscode-memoryops
npm install
npm run compile
npm test
```

Then open this folder in VS Code and press `F5` to launch an Extension Development Host.

## Package Locally

```bash
cd extensions/vscode-memoryops
npm install
npm run package
```

This creates a local `.vsix` package. Marketplace publishing is intentionally out of scope for this scaffold.

## Roadmap

- Optional chat participant integration for VS Code native chat workflows.
- Extension Host tests for command wiring and end-to-end sidebar flows.
- Marketplace packaging metadata, icon, screenshots, and release workflow.
