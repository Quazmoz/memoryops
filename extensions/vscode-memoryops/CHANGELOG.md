# Changelog

All notable changes to the MemoryOps VS Code extension will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [1.0.3] - 2026-06-08

### Changed

- **Versioned Skill Testing and Invocation Support** — updated the client library to accept an optional version parameter in `testSkill` and `invokeSkill` API requests, supporting versioned skill execution backend capabilities.

## [1.0.2] - 2026-06-08

### Added

- **Sidebar Skills Tree View** — added a new Skills tree view to the sidebar panel listing all registered workspace skills, including inline toggle, test, history, and delete actions, plus a header bar button to create new skills directly from the UI.
- **Contextual Command Arguments** — updated the skills commands to run directly on the selected sidebar tree node.

## [1.0.1] - 2026-06-05

### Fixed

- **Sidebar script boot failure** — fixed a broken inline webview script escape sequence that made the sidebar JavaScript fail to parse. This is why the inner Refresh / Settings buttons and `Open Settings` CTA stopped working and the sidebar stayed on `Loading...`.
- **Regression coverage** — added a build-time test that parses the generated inline webview script so escaping bugs in the embedded HTML/JS are caught before packaging.

## [1.0.0] - 2026-06-04

### Fixed

- **Uninstall cleanup** — the extension now registers VS Code's official `vscode:uninstall` hook and removes persisted `memoryops.*` settings plus tracked extension storage on uninstall.
- **Legacy plaintext API key cleanup** — when a secure API key is stored in VS Code SecretStorage, any old `memoryops.apiKey` fallback entries are removed so stale keys do not linger in `settings.json`.
- **Status bar reconnect drift** — the status bar now restores the correct click action after failures instead of sometimes looking healthy while still pointing at `Reconnect`.

## [0.4.0] - 2026-06-04

### Added

- **Skills management commands** — register and manage HTTP Skills directly from the Command Palette: `MemoryOps: List Skills`, `Create Skill`, `Toggle Skill Enabled`, `Delete Skill`, `Test Skill`, `View Skill Version History`, and `Roll Back Skill Version`.
- **Skill versioning** — every create/update bumps the skill version and snapshots a full history entry. `View Skill Version History` opens a markdown report; `Roll Back Skill Version` lets you pick any prior version, add a change note, and restore it as a new bumped version.

## [0.3.1] - 2026-06-03

### Fixed

- **Sidebar stuck on "Loading…" and dead toolbar buttons** — the webview now declares a `Content-Security-Policy` with a per-render nonce and runs its script under it. Previously the inline script and inline `onclick` handlers were unguarded, so in stricter editor environments the view could fail to receive state (stuck on "Loading…" after entering API key / workspace ID) and the Refresh / Settings (and card action) buttons did nothing. All handlers now use `addEventListener` + event delegation, and the `message` listener is registered before the `ready` handshake so the first state push can't be dropped.

## [0.3.0] - 2026-06-03

### Added

- **Getting Started walkthrough** — a guided, four-step onboarding (connect backend → authenticate → test connection → explore) that opens automatically on first install when the extension is not yet configured. Re-openable via `MemoryOps: Open Getting Started Walkthrough`.
- **`@memoryops` Copilot Chat participant** — query your workspace conversationally from Copilot Chat. Supports `/search` (default) and `/retrieve` (packed, token-budgeted context) slash commands, with "Open in editor" buttons and follow-up suggestions. No-op when no chat-capable client is installed.
- **Inline CodeLens hints** — opt-in (`memoryops.enableCodeLens`) lens at the top of files showing an **exact** count of memories that reference the current file; click to surface them. Backed by a new backend `source_ref` list filter (matches memories by the file recorded on their originating observation, ignoring line anchors) rather than a fuzzy filename search. Results are cached per file for 60s.
- **`MemoryOps: Reconnect`** — drop the cached client and re-establish the connection. Connection failures now offer **Reconnect** / **Open Settings** actions, and the status bar item becomes a one-click reconnect.
- **`MemoryOps: Show Memories Referencing Current File`** — find memories that reference the active file (exact `source_ref` match, not fuzzy search).

### Changed

- **Automatic retry with exponential backoff** — read-only requests (search, retrieve, list, health) are retried on transient failures (timeouts, network errors, 5xx/429). Mutating writes are never auto-retried. Tunable via `memoryops.maxRetries` and `memoryops.retryBackoffMs`.
- **Command Palette hygiene** — selection/editor-scoped commands (`Save Selection as Observation`, `Retrieve Context for Current File`, `Insert Retrieval Context`, `Show Memories Referencing Current File`) now only appear when an editor is open / has a selection.

## [0.2.0] - 2026-06-03

### Added

- **Marketplace release** — first public release on the VS Code Marketplace.
- Rich webview sidebar with card-based memory browser, inline search, tabs (All / Episodic / Semantic / Pinned), and pagination.
- `MemoryOps: Search Memory` — hybrid, keyword, or vector search from the Command Palette.
- `MemoryOps: Retrieve Context for Current File` — token-budgeted context retrieval for the active file or selection.
- `MemoryOps: Save Selection as Observation` — save selected code or notes as a new episodic memory.
- `MemoryOps: Edit Memory` — edit content (in a full editor buffer), tags, and importance score.
- `MemoryOps: Promote Memory` — promote episodic memories to semantic.
- `MemoryOps: Publish Memory To Workspace` — publish semantic memories to the workspace pool.
- `MemoryOps: Merge Memory` — merge two semantic memories.
- `MemoryOps: View Memory History` — inspect version history.
- `MemoryOps: View Memory Provenance` — view provenance graph.
- `MemoryOps: View Memory Feedback` — view retrieval feedback entries.
- `MemoryOps: Submit Retrieval Feedback` — rate retrieved memory relevance.
- `MemoryOps: Insert Retrieval Context` / `Copy Retrieval Context` — insert or copy packed context.
- `MemoryOps: Filter and Sort Sidebar` — filter by type, pinned status; sort by importance, decay, relevance, or date.
- `MemoryOps: Bulk Operations` — pin, unpin, or delete multiple memories at once.
- Pin/unpin, delete, and copy memory content from sidebar context menus.
- Status bar indicator with connection health.
- Automatic sidebar refresh on configuration changes.

## [0.1.1] - 2026-06-01

### Fixed

- Minor packaging and metadata fixes.

## [0.1.0] - 2026-05-30

### Added

- Initial scaffold with core commands and sidebar tree provider.

## [0.0.1] - 2026-05-28

### Added

- Project scaffolding.
