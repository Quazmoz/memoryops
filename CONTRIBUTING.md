# Contributing to MemoryOps

Thank you for your interest in contributing. This document covers how to get set up, the project conventions, and the PR process.

---

## Table of Contents

- [Getting Started](#getting-started)
- [Development Setup](#development-setup)
- [Project Structure](#project-structure)
- [Making Changes](#making-changes)
- [Pull Request Process](#pull-request-process)
- [Coding Conventions](#coding-conventions)
- [Commit Messages](#commit-messages)
- [Reporting Bugs](#reporting-bugs)
- [Feature Requests](#feature-requests)

---

## Getting Started

1. Fork the repository and clone your fork.
2. Follow the [Quick Start](README.md#quick-start) in the README to get a local environment running.
3. Verify the test suite passes before making any changes:
   ```bash
   cargo test --workspace
   ```

---

## Development Setup

See [docs/local-development.md](docs/local-development.md) for the full step-by-step guide.

**Key tools:**

```bash
# Rust toolchain (version pinned in rust-toolchain.toml)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# sqlx CLI (for migrations)
cargo install sqlx-cli --no-default-features --features rustls,postgres

# cargo-watch (optional, live reload during development)
cargo install cargo-watch

# Frontend
cd frontend && npm install
```

**Start the test stack:**

```bash
docker compose -f docker-compose.test.yml up -d
cargo test --workspace
```

---

## Project Structure

MemoryOps is a Cargo workspace. Each crate has a single responsibility:

| Crate | Purpose |
|-------|---------|
| `common` | Shared types, DB models, config, provider traits (`LlmProvider`, `EmbeddingProvider`), `AppError`, `AppState` |
| `api` | Axum HTTP handlers, middleware, routing. Depends on `common` and `retrieval`. |
| `ingestion` | Webhook receivers for GitHub, Slack, Jira, Linear. Pushes events to the Redis queue. |
| `processor` | Fast-path and slow-path (async LLM) workers. Consumes from Redis, writes to Postgres + Qdrant. |
| `retrieval` | Hybrid search (Qdrant + Tantivy), RRF scoring, token packing, feedback integration. |
| `mcp` | MCP server exposing `memory_retrieve`, `memory_search`, `memory_store`, and related tools. |

Cross-crate dependency rule: **`common` depends on nothing internal. All other crates may depend on `common`. Crates should not depend on each other except `api` → `retrieval`.**

---

## Making Changes

1. Create a feature branch from `main`:
   ```bash
   git checkout -b feat/your-feature-name
   ```
2. Make your changes. Keep commits focused and atomic.
3. Add or update tests for any logic changes.
4. Run the full check suite locally before pushing:
   ```bash
   cargo fmt --all
   cargo clippy --workspace -- -D warnings
   cargo test --workspace
   ```
5. If you changed `.env.example` or `config.toml`, update the relevant docs table in `README.md`.
6. If you added a new LLM or embedding provider, add an entry to [docs/PROVIDERS.md](docs/PROVIDERS.md).

---

## Pull Request Process

- Open PRs against `main`.
- Fill in the PR template — description, motivation, test coverage, and any breaking changes.
- CI must pass (fmt, clippy, tests) before review.
- One approving review required from a maintainer.
- Squash merge is preferred for feature branches; rebase merge for small fixes.

**Breaking changes** (API surface, config schema, migration changes) must be clearly flagged in the PR description and will require a minor version bump.

---

## Coding Conventions

- **Formatting:** `cargo fmt` — enforced by CI. Config in `.rustfmt.toml`.
- **Linting:** `cargo clippy -- -D warnings` — zero warnings policy.
- **Error handling:** All fallible public functions return `AppResult<T>` (alias for `Result<T, AppError>`). Use `?` propagation; never `unwrap()` in non-test code.
- **Async:** All async code uses `tokio`. Do not block the async runtime — offload CPU-bound work to `tokio::task::spawn_blocking`.
- **Database:** Use `sqlx` query macros with compile-time checking. Run `cargo sqlx prepare` after changing queries.
- **Secrets:** Never hardcode secrets. All credentials are read from environment variables via `config.rs` resolver methods.
- **Tests:** Unit tests live in `#[cfg(test)]` modules in the same file. Integration tests live in `tests/` under each crate. Use `docker-compose.test.yml` for integration test infrastructure.

---

## Commit Messages

Follow [Conventional Commits](https://www.conventionalcommits.org/):

```
feat(retrieval): add cosine deduplication threshold config
fix(api): return 404 instead of 500 for missing workspace
chore(deps): bump tokio to 1.38
docs: add OpenRouter provider example to PROVIDERS.md
test(processor): add slow-path LLM summarization integration test
```

Types: `feat`, `fix`, `docs`, `test`, `chore`, `refactor`, `perf`, `ci`.

---

## Reporting Bugs

Open a [GitHub Issue](https://github.com/Quazmoz/memoryops/issues/new?template=bug_report.md) using the bug report template. Include:

- MemoryOps version / commit SHA
- Rust version (`rustc --version`)
- Reproduction steps
- Expected vs. actual behavior
- Relevant logs (`RUST_LOG=debug`)

---

## Feature Requests

Open a [GitHub Issue](https://github.com/Quazmoz/memoryops/issues/new?template=feature_request.md) using the feature request template. Check [docs/FEATURES.md](docs/FEATURES.md) first — your idea may already be on the roadmap.
