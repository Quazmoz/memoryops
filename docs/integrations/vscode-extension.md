# MemoryOps VS Code Extension

MemoryOps includes an early VS Code extension scaffold under:

```text
extensions/vscode-memoryops
```

The extension is not published to the Visual Studio Marketplace yet. It is intended for local development, dogfooding, and iteration before Marketplace packaging.

## Current MVP

The scaffold currently includes these commands:

| Command | Purpose |
|--------|---------|
| `MemoryOps: Test Connection` | Checks API readiness and workspace access. |
| `MemoryOps: Search Memory` | Searches the configured MemoryOps workspace from the Command Palette. |
| `MemoryOps: Retrieve Context for Current File` | Sends the active file/selection as retrieval context and opens returned memory context in a Markdown preview document. |
| `MemoryOps: Save Selection as Observation` | Sends selected code or notes to `/v1/ingest/observation`. |
| `MemoryOps: Open Settings` | Opens MemoryOps extension settings. |

## Settings

Configure the extension in VS Code user settings:

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

Store `memoryops.apiKey` in user settings. Do not commit API keys to repository-level `.vscode/settings.json`.

## Local development

```bash
cd extensions/vscode-memoryops
npm install
npm run compile
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

- Sidebar/webview for richer memory browsing.
- Git remote detection for better repo-aware retrieval.
- Optional VS Code chat participant integration.
- Memory pin/delete actions from result views.
- Command and API-client tests.
- Marketplace packaging metadata, icon, screenshots, and release workflow.
