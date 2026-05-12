# MemoryOps VS Code Extension

> Early scaffold for bringing governed MemoryOps workspace memory into VS Code.

This extension is not published to the Visual Studio Marketplace yet. It is included in the MemoryOps repo as an MVP scaffold for local development and future packaging.

## Current MVP Features

- `MemoryOps: Test Connection` — checks API readiness and workspace access.
- `MemoryOps: Search Memory` — searches workspace memory from the Command Palette.
- `MemoryOps: Retrieve Context for Current File` — requests context for the active file/selection.
- `MemoryOps: Save Selection as Observation` — sends selected code or notes to `/v1/ingest/observation`.
- `MemoryOps: Open Settings` — opens extension settings.

## Settings

```json
{
  "memoryops.apiUrl": "http://localhost:8080",
  "memoryops.workspaceId": "<workspace-uuid>",
  "memoryops.apiKey": "<mops-api-key>",
  "memoryops.defaultTopK": 5,
  "memoryops.defaultTokenBudget": 2048,
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

- Sidebar/webview for richer memory results.
- Better repo detection using Git remotes instead of workspace folder name.
- Optional chat participant integration for VS Code native chat workflows.
- Memory pin/delete actions from result views.
- Tests for command registration and client behavior.
