# MemoryOps Production Security

MemoryOps is a self-hosted memory control plane for AI agents. Treat it as sensitive infrastructure: it stores workspace memory, retrieval traces, API keys, encrypted tool and webhook secrets, provider key references, audit data, and MCP access to read/write operations.

## Threat Model Summary

Primary threats:

- Stolen workspace API keys or bootstrap admin tokens.
- Accidental public exposure of Postgres, Redis, Qdrant, MCP, or internal tool endpoints.
- SSRF through workspace tool URLs or redirected outbound calls.
- Webhook spoofing, replay, or oversized payload abuse.
- Secret disclosure through logs, exports, frontend examples, support bundles, or container bind mounts.
- Cross-workspace access caused by missing workspace checks.
- Supply-chain compromise of Rust, npm, Docker, or GitHub Actions dependencies.
- Backup theft or loss of `APP_SECRET_KEY`, making encrypted tool/webhook secrets unrecoverable.

## Assets Protected

- Workspace API keys and key prefixes.
- Workspace data and memory content.
- Retrieval traces and raw ingestion payloads.
- Encrypted tool secrets.
- Webhook signing secrets.
- LLM and embedding provider keys referenced by environment variable.
- Audit data and compliance events.
- MCP access to memory/tool operations.
- Postgres, Redis, and Qdrant data volumes and backups.

## Trust Boundaries

- Browser/frontend: untrusted user input, served by nginx, proxies `/api` to the backend.
- API: authentication, authorization, rate limiting, audit, encryption, and outbound tool invocation boundary.
- MCP server: authenticated agent interface; powerful enough to read/write memory and invoke workspace tools.
- Postgres: system of record for workspaces, keys, audit entries, integrations, traces, memories, and encrypted secrets.
- Redis: rate-limit state and ingestion/processor queues.
- Qdrant: vector index for memory retrieval.
- Webhook providers: GitHub, Slack, Jira, Linear, and generic observation ingestion.
- External HTTP tools: workspace-configured endpoints; blocked from private/internal ranges by default.
- LLM/embedding providers: external processors that may receive memory content.
- Reverse proxy/load balancer: TLS termination and trusted source of client IP headers.

## Production Checklist

- Terminate HTTPS at a reverse proxy and redirect HTTP to HTTPS.
- Configure HSTS at the reverse proxy after confirming the hostname is HTTPS-only.
- Do not expose container ports directly to the internet; expose only the reverse proxy.
- Keep Postgres, Redis, and Qdrant internal-only. The production compose overlay removes their host bindings.
- Keep MCP loopback-only or private-network-only. Do not publish MCP to the internet unless it is behind explicit authentication, VPN/private networking, and firewall allowlists.
- Set `APP_ENV=production`.
- Set a long random `APP_SECRET_KEY`; keep it stable across restarts.
- Set `WORKSPACE_CREATION_ENABLED=true` only during initial bootstrap, then set it to `false`.
- Rotate or remove `WORKSPACE_CREATION_SECRET` after bootstrap, and firewall `POST /v1/workspaces`.
- Use real database, Redis, Qdrant, and provider credentials. Do not reuse local compose defaults outside development.
- Configure `TRUSTED_PROXY_CIDRS` to only the reverse proxy/load balancer CIDRs. Leave empty when there is no trusted proxy.
- Keep CORS/reverse-proxy origin policy same-origin unless you intentionally deploy a separate frontend origin.
- Keep `[server].allow_private_ips = false` unless MemoryOps runs in a single-tenant private network and internal tool calls are required.
- Encrypt backups and test restores regularly.
- Document log retention and audit retention.
- Rotate workspace API keys regularly and immediately after suspected disclosure.
- Run dependency scanning (`cargo audit`, `npm audit`), secret scanning (`gitleaks` or `trufflehog`), and container image scanning before release.
- Review the frontend nginx security headers after any change that introduces inline scripts, third-party assets, or external API origins.

## Container Hardening

Use:

```bash
docker compose -f docker-compose.yml -f docker-compose.prod.yml up -d
```

The production overlay:

- Removes public bindings for Postgres, Redis, and Qdrant.
- Binds API, frontend, and MCP to `127.0.0.1` for a local reverse proxy.
- Defaults `WORKSPACE_CREATION_ENABLED=false`.
- Drops Linux capabilities and sets `no-new-privileges` on API, MCP, and frontend.
- Runs API, MCP, and frontend with read-only root filesystems and `/tmp` tmpfs scratch space.
- Clears local-dev `.gemini` and `.claude` bind mounts for API/MCP.

Postgres, Redis, and Qdrant need writable data volumes, so they are not configured as read-only containers.

## Reverse Proxy Example

Caddy:

```caddyfile
memoryops.example.com {
  encode zstd gzip
  request_body {
    max_size 10MB
  }

  header {
    Strict-Transport-Security "max-age=31536000; includeSubDomains"
  }

  reverse_proxy 127.0.0.1:5173 {
    header_up X-Forwarded-Proto {scheme}
    header_up X-Forwarded-For {remote_host}
    header_up X-Real-IP {remote_host}
  }
}
```

Set `TRUSTED_PROXY_CIDRS` to the proxy host or load-balancer CIDRs, for example `127.0.0.1/32` for local Caddy/nginx.

## Authentication And Bootstrap

- Normal API calls require `X-API-Key`.
- API keys are generated as `mops_...`, stored as Argon2 hashes, and returned only once.
- Revoked keys are excluded from lookup and their Redis auth cache entry is invalidated.
- `POST /v1/workspaces` requires `X-Admin-Token: <workspace-creation-secret>`.
- `WORKSPACE_CREATION_ENABLED=false` disables workspace creation at the API layer.
- First-key bootstrap for an existing workspace is only allowed when that workspace has zero active API keys.

Recommended bootstrap sequence:

1. Set `APP_ENV=production`, `APP_SECRET_KEY`, `WORKSPACE_CREATION_SECRET`, and temporarily `WORKSPACE_CREATION_ENABLED=true`.
2. Create the initial workspace and store the returned API key in a secrets manager.
3. Set `WORKSPACE_CREATION_ENABLED=false`.
4. Rotate or remove `WORKSPACE_CREATION_SECRET`.
5. Restrict `POST /v1/workspaces` at the reverse proxy or firewall.

## Rate Limiting

- Unauthenticated ingestion and bootstrap paths are IP-limited.
- Authenticated retrieval/workspace/API paths are workspace-limited after authentication.
- Redis failures in protected rate-limit checks fail closed with `429`.
- Configure Redis as a required production dependency, not an optional cache.

## SSRF And Outbound Tools

Workspace tool URLs:

- Must use HTTPS.
- Must not include URL credentials.
- Reject `localhost`, loopback, link-local, RFC1918 private ranges, metadata IPs, Docker bridge/private ranges, CGNAT, IPv4-mapped internal IPv6, IPv6 unique-local, IPv6 link-local, and documentation ranges by default.
- Are DNS-resolved before invocation, and redirects are disabled.

Escape hatch:

- `[server].allow_private_ips = true` allows private/internal tool endpoints.
- Use it only in private, single-tenant deployments where tool targets are controlled.
- Prefer explicit network allowlists and egress firewall rules even when the app-level guard is enabled.

## Webhooks

- GitHub, Slack, Jira, and Linear webhooks use HMAC-SHA256 verification.
- Slack includes timestamp skew validation for replay protection.
- GitHub uses delivery IDs as idempotency keys.
- Jira and Linear provider replay protection is limited to idempotency and stored event semantics; use provider-side retry controls and short-lived secrets where available.
- Webhook request bodies are explicitly limited to 1 MiB.
- Webhook secrets are encrypted and must never be logged or sent to clients.

## MCP

MCP transports require bearer API key authentication. MCP exposes read/write memory operations and tool invocation, so treat it as privileged.

Recommended access patterns:

- stdio for local desktop/CLI clients.
- Loopback HTTP for local Open WebUI/VS Code style clients.
- VPN, Tailscale, WireGuard, SSH tunnel, or a private subnet for remote use.

Do not expose MCP publicly without a strong network access policy and monitoring.

## Secrets And Logging

- Do not commit `.env`, MCP client configs, generated secret files, or local runtime credential files.
- Use a secrets manager in production.
- API keys and auth headers must not be logged.
- Tool/webhook secrets are encrypted with `APP_SECRET_KEY`.
- Tool secret reveal is sensitive and audited as `tool_secret_revealed`; the audit diff records metadata only, not the secret value.
- Export/list endpoints should not include plaintext tool secrets.
- Audit logging, redaction, tamper-evidence (`AUDIT_SIGNING_KEY`), and retention (`AUDIT_RETENTION_DAYS`) are documented in [docs/audit.md](audit.md). Security-sensitive actions use reliable (synchronous) audit writes; secret values are redacted before persistence.

`APP_SECRET_KEY` rotation caveat: rotating it requires re-encrypting stored tool and webhook secrets or keeping the old key available for a migration. Do not rotate it casually until a migration plan exists.

## Database, Redis, Qdrant, And Backups

- Use a least-privilege Postgres role for `DATABASE_URL`.
- Use `sslmode=require` or stronger when connecting to managed Postgres outside the Docker network.
- Encrypt Postgres, Redis, and Qdrant volumes/backups at rest.
- Test restore procedures on a schedule.
- Protect Redis from public access; it holds queues and rate-limit state.
- Protect Qdrant from public access; it holds searchable memory vectors.
- Verify volume ownership and permissions after deployment.

## Supply Chain

- Run `cargo fmt`, `cargo clippy`, and `cargo test` before release.
- Run `cargo audit`.
- Run `npm audit` for `frontend` and `extensions/vscode-memoryops`.
- Run `gitleaks detect` or the `Secret Scan` workflow.
- Scan built images with tools such as Trivy, Grype, or your registry scanner.
- Review Dependabot PRs for Cargo, npm, Docker, and GitHub Actions.
- Pin image tags and avoid `latest`.

