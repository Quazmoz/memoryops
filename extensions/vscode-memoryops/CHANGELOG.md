# Changelog

All notable changes to the MemoryOps VS Code extension will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

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
