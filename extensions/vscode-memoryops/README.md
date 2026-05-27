# MemoryOps VS Code Extension

> Early scaffold for bringing governed MemoryOps workspace memory into VS Code.

This extension is not published to the Visual Studio Marketplace yet. It is included in the MemoryOps repo for local development, dogfooding, and future packaging.

## Current Features

- `MemoryOps: Test Connection` — checks API readiness and workspace access.
- `MemoryOps: Refresh Memories` — loads recent workspace memories into the MemoryOps sidebar.
- `MemoryOps: Search Memory` — searches workspace memory from the Command Palette.
- `MemoryOps: Retrieve Context for Current File` — requests context for the active file/selection.
- `MemoryOps: Save Selection as Observation` — sends selected code or notes to `/v1/ingest/observation`.
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
- Tests for command registration and client behavior.
- Marketplace packaging metadata, icon, screenshots, and release workflow.
