# Security Policy

## Supported Versions

MemoryOps is currently in alpha. Security fixes are applied to the latest commit on `main` only.

| Version | Supported |
|---------|----------|
| `main` (alpha) | ✅ |
| Older tagged releases | ❌ |

---

## Reporting a Vulnerability

**Please do not open a public GitHub issue for security vulnerabilities.**

Report vulnerabilities privately via [GitHub Security Advisories](https://github.com/Quazmoz/memoryops/security/advisories/new).

Include:
- Description of the vulnerability and potential impact
- Steps to reproduce or proof-of-concept
- Any suggested mitigations

You will receive an acknowledgement within 72 hours. We aim to triage and release a fix within 14 days for critical issues.

---

## Security Considerations

For deployment-specific hardening, use the production checklist in
[`docs/security-production.md`](docs/security-production.md).

### API Keys

- Workspace API keys are hashed with Argon2 before storage. The plaintext key is returned **once** on creation — it cannot be recovered.
- Bootstrap keys (`POST /v1/workspaces`) have elevated privileges. Rotate them after initial setup using `POST /v1/workspaces/{id}/keys`.
- Keys are passed via `X-API-Key` header — always use HTTPS in production.

### Webhook Validation

- All webhook sources (GitHub, Slack, Jira, Linear) are validated with HMAC-SHA256 signatures.
- The `dev-placeholder` secret in `.env.example` is intentionally weak. **Always set real secrets in production.**
- Timing-safe comparison is used for all HMAC checks.

### Secrets Management

- No secrets are stored in `config.toml`. All credentials are read from environment variables at runtime.
- Never commit `.env` to source control. `.env` is in `.gitignore`.
- In production, use a secrets manager (AWS Secrets Manager, HashiCorp Vault, etc.) to inject environment variables rather than a `.env` file.

### Network Exposure

- The bootstrap endpoint (`POST /v1/workspaces`) should be restricted to trusted networks and disabled with `WORKSPACE_CREATION_ENABLED=false` after initial setup.
- The MCP server port (default `3003`) should not be exposed publicly — it is intended for local AI client connections only.
- Qdrant and Redis should not be exposed to the public internet. Bind them to `localhost` or a private network interface.

### Database

- Use a dedicated Postgres role with least-privilege permissions for the `DATABASE_URL` connection.
- Enable Postgres SSL (`sslmode=require`) in production.
- All user-supplied values are parameterized via `sqlx` — no string interpolation in SQL.

### LLM Provider Keys

- LLM and embedding API keys (`OPENAI_API_KEY`, `ANTHROPIC_API_KEY`, etc.) are read from environment variables and never logged or persisted to the database.
- Audit your LLM provider's data retention policies if you are processing sensitive memory content.
