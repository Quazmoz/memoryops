# MemoryOps Production Deployment Guide

This guide details the deployment strategies, configuration options, and architectural practices for running MemoryOps in production environments, such as Docker, Kubernetes (K8s), or K3s.

## Docker Hub Images

Pre-built multi-platform images (`linux/amd64`, `linux/arm64`) are published to [Docker Hub](https://hub.docker.com/r/quazmoz/memoryops):

| Image | Tag | Description |
|-------|-----|-------------|
| `quazmoz/memoryops` | `api-latest` | API server (Rust/axum) |
| `quazmoz/memoryops` | `mcp-latest` | MCP gateway (Rust) |
| `quazmoz/memoryops` | `frontend-latest` | Control UI (React/nginx) |

Versioned tags follow the pattern `api-0.1.0`, `mcp-0.1.0`, `frontend-0.1.0`.

```bash
# Pull all images
docker pull quazmoz/memoryops:api-latest
docker pull quazmoz/memoryops:mcp-latest
docker pull quazmoz/memoryops:frontend-latest
```

---

## Deployment Architecture

In a production environment, the MemoryOps services are decoupled into stateless application layers and stateful persistence layers to ensure high availability, horizontal scalability, and resilience.

```
                  ┌───────────────────────┐
                  │    Ingress / Load     │
                  │       Balancer        │
                  └──────────┬────────────┘
                             │ HTTP / gRPC
         ┌───────────────────┼───────────────────┐
         ▼                   ▼                   ▼
┌─────────────────┐ ┌─────────────────┐ ┌─────────────────┐
│   API Pod 1     │ │   API Pod 2     │ │   API Pod N     │  (Stateless API replicas)
└────────┬────────┘ └────────┬────────┘ └────────┬────────┘
         │                   │                   │
         └─────────────┬─────┴─────────────┬─────┘
                       │ Postgres          │ Redis / Qdrant
                       ▼                   ▼
             ┌───────────────────┐       ┌───────────────────┐
             │ Postgres Database │       │   Redis / Qdrant  │  (Stateful Storage / Vector Search)
             │      Cluster      │       │     Services      │
             └───────────────────┘       └───────────────────┘
```

1. **Stateless API Layer (`api`)**: The core Rust application containing the Axum web server and worker pools. These processes handle ingest, query, and background processors (clustering, decay). They do not maintain state on local disk and can scale horizontally.
2. **Stateless MCP Gateway (`mcp`)**: Optional gateway facilitating communication using Model Context Protocol (MCP) transport.
3. **Stateless Frontend Layer (`frontend`)**: The static SPA bundled with Vite, served via an embedded Nginx/web server container.
4. **Stateful Services Layer**: Postgres (relational/metadata storage), Redis (event queue and job locking), and Qdrant (vector search database). These should be backed by persistent storage or managed cloud services (e.g., AWS RDS, ElastiCache, Qdrant Cloud).

---

## Kubernetes & K3s Deployment Strategy

When deploying to Kubernetes or K3s, follow these best practices for replica scaling and database operations.

### 1. Database Migrations (Preventing races)
By default, the backend API container attempts to run database migrations on boot. In a horizontally scaled deployment, starting multiple replicas simultaneously causes migrations to race and fail due to lock contention.

To prevent this:
1. Set the environment variable `SKIP_MIGRATIONS=true` on your main API Deployment specs.
2. Run database migrations exactly once per deployment using a Kubernetes **Job** or an **Init Container** that executes before the API pods launch.

**Example Migration Job Spec (`memoryops-db-migrate.yaml`):**
```yaml
apiVersion: batch/v1
kind: Job
metadata:
  name: memoryops-db-migrate
  namespace: memoryops
spec:
  template:
    spec:
      restartPolicy: OnFailure
      containers:
        - name: migrate
          image: quazmoz/memoryops:api-latest
          command: ["/usr/local/bin/api"] # Triggers migrations when SKIP_MIGRATIONS is false
          env:
            - name: DATABASE_URL
              valueFrom:
                secretKeyRef:
                  name: memoryops-secrets
                  key: database-url
            - name: SKIP_MIGRATIONS
              value: "false" # Explicitly run migrations in this job
            - name: REDIS_URL
              value: "redis://memoryops-redis:6379"
            - name: QDRANT_URL
              value: "http://memoryops-qdrant:6334"
            - name: APP_SECRET_KEY
              valueFrom:
                secretKeyRef:
                  name: memoryops-secrets
                  key: app-secret-key
            - name: WORKSPACE_CREATION_SECRET
              valueFrom:
                secretKeyRef:
                  name: memoryops-secrets
                  key: workspace-creation-secret
```

### 2. Horizontally Scaling Stateless API Replicas
Since the Agent Library is **database-backed** (stored in the versioned `agent_resources` tables, with `agent_skills` retained for compatibility), individual API replicas do not require local persistent volumes (PVs) or directory mounts for `.gemini/skills` or `.claude/skills`.
You can safely set the `replicas` count to `2+` in your Deployment manifest.

**Example API Deployment Spec (`memoryops-api-deployment.yaml`):**
```yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: memoryops-api
  namespace: memoryops
spec:
  replicas: 3
  selector:
    matchLabels:
      app: memoryops-api
  template:
    metadata:
      labels:
        app: memoryops-api
    spec:
      containers:
        - name: api
          image: quazmoz/memoryops:api-latest
          ports:
            - containerPort: 8080
          readinessProbe:
            httpGet:
              path: /health/ready
              port: 8080
            initialDelaySeconds: 5
            periodSeconds: 10
          livenessProbe:
            httpGet:
              path: /health
              port: 8080
            initialDelaySeconds: 15
            periodSeconds: 20
          env:
            - name: SKIP_MIGRATIONS
              value: "true" # Skip migrations on boot for replicas
            - name: DATABASE_URL
              valueFrom:
                secretKeyRef:
                  name: memoryops-secrets
                  key: database-url
            - name: REDIS_URL
              value: "redis://memoryops-redis:6379"
            - name: QDRANT_URL
              value: "http://memoryops-qdrant:6334"
            - name: QDRANT_CHECK_COMPATIBILITY
              value: "false" # Bypass client-server compatibility check
            - name: APP_SECRET_KEY
              valueFrom:
                secretKeyRef:
                  name: memoryops-secrets
                  key: app-secret-key
            - name: WORKSPACE_CREATION_SECRET
              valueFrom:
                secretKeyRef:
                  name: memoryops-secrets
                  key: workspace-creation-secret
            - name: APP_ENV
              value: "production"
            - name: WORKSPACE_CREATION_ENABLED
              value: "false"
            - name: TRUSTED_PROXY_CIDRS
              value: "10.0.10.0/24" # Use only ingress/reverse-proxy CIDRs, not the whole VPC
```

---

## Production Security & Network Configuration

### 1. Enforcing Production Mode (`APP_ENV=production`)
When the environment variable `APP_ENV` is set to `production`, the backend enforces strict validation on startup:
* **Secret Key Validation**: The API will crash on boot if `APP_SECRET_KEY` is set to the development fallback value `dev-placeholder`. If workspace creation is enabled, `WORKSPACE_CREATION_SECRET` must also be a real value.
* **Workspace Creation Switch**: Set `WORKSPACE_CREATION_ENABLED=false` after initial bootstrap so `POST /v1/workspaces` is rejected even if the admin token leaks.
* **Database & Encryption**: These values must be long, randomly generated secrets securely stored in a KMS (Key Management Service) or Kubernetes Secret and injected at runtime.

### 2. SSRF & Private IP Whitelisting (`allow_private_ips`)
By default, MemoryOps rejects custom skill tools targeting private IP addresses (such as RFC-1918 blocks, link-local metadata endpoints, and loopback) to prevent Server-Side Request Forgery (SSRF).

If you deploy MemoryOps in a private VPC or behind a secure tunnel (e.g. Cloudflare Tunnels, tailscale) and need the server to call tool endpoints running on internal IPs:
* In `config.toml` under `[server]`, configure:
  ```toml
  allow_private_ips = true
  ```
* Or set the environment variable:
  ```bash
  MEMORYOPS_ALLOW_PRIVATE_IPS=true
  ```

> [!WARNING]
> Only enable `allow_private_ips` in secure, single-tenant, private networking environments. In public multi-tenant deployments, keep it set to `false`.

### 3. Client IP Extraction (`TRUSTED_PROXY_CIDRS`)
When behind an Ingress Controller (like NGINX, Traefik, or an AWS ALB), the user's IP is forwarded using the `X-Forwarded-For` (XFF) header. To prevent IP spoofing, configure `TRUSTED_PROXY_CIDRS` with the CIDR ranges of your ingress controllers. Only headers sent from these CIDRs will be parsed.

---

## Hardened Docker Compose Production Setup

For running in plain Docker environments, MemoryOps uses a multi-file composition approach.

* **Development/Local** (`docker-compose.yml`): Binds ports for databases (5432, 6379, 6334) to loopback `127.0.0.1` so you can connect local developer tools directly.
* **Production Hardened Overlay** (`docker-compose.prod.yml`): Modifies the base compose layout for production.

### Using the Production Overlay
Run the composition by merging the configurations:
```bash
docker compose -f docker-compose.yml -f docker-compose.prod.yml up -d
```

### Key Differences in Production:
1. **Network Isolation**: The databases (postgres, redis, qdrant) have their port bindings completely removed using the `!reset []` directive. This prevents them from binding to host interfaces, isolating them strictly within the Docker bridge network.
2. **Host Binding Limits**: The `api`, `mcp`, and `frontend` containers bind their listening ports strictly to loopback `127.0.0.1`.
3. **Reverse Proxy Dependency**: Place a reverse proxy (e.g., Caddy, NGINX, or Traefik) directly on the host machine to handle SSL termination, routing traffic from the public network to the internal loopback ports (`8080` for API, `5173` for Frontend).

---

## Production AI & LLM Provider Configuration

In a remote server setup, you must configure how the API server reaches your LLM and Embedding models. See [LLM & Embedding Provider Configuration](PROVIDERS.md) for individual provider parameters.

### 1. Connecting to Local / Private Ollama GPU Servers
If you host Ollama in a separate container or on a GPU-enabled host machine:
* **Docker Desktop**: Set `llm.base_url = "http://host.docker.internal:11434"`
* **Linux Container Engine**: Since `host.docker.internal` is not automatically mapped on Linux, add the host gateway resolution in your compose configuration:
  ```yaml
  services:
    api:
      extra_hosts:
        - "host.docker.internal:host-gateway"
  ```
* **Remote VM / Shared Cluster**: Set `llm.base_url` to the private domain name or IP of your GPU instance (e.g., `http://ollama-service.internal:11434`).

### 2. Standardizing Embeddings
If you choose to use the local `fastembed` provider (`provider = "fastembed"`), the Rust API downloads the BAAI BGE model on first launch. If your production pods run in a restricted or offline network:
* Consider switching to `openai` embeddings (`provider = "openai"`) which query a remote HTTPS API.
* Or prepopulate the model cache directory inside your Docker image during build time (defaulting to the cache dir of `fastembed-rs`).

---

## Managing Agent Library Resources on Remote Servers

Agent skills, agent profiles, prompts, and reusable instructions are stored directly in PostgreSQL (`agent_resources` and `agent_resource_versions`) scoped by `workspace_id`. This guarantees database consistency, preserves immutable version history, and removes file synchronization issues across stateless API replicas. The legacy `agent_skills` API remains available for Claude/Gemini skill sync workflows.

### 1. Workspace Skill Seeding
When a workspace is created, or when listing/retrieving an Agent Library kind that has `0` resources, the server seeds safe starter resources without overwriting existing rows. Skill defaults come from the server filesystem's `.gemini/skills/` and `.claude/skills/` directories; prompts, agent profiles, and reusable instructions are seeded from built-in MemoryOps defaults with version history.
The default skill markdown files are packed into the production Docker image during the build stage (`COPY .gemini /app/.gemini` and `COPY .claude /app/.claude`).

### 2. Synchronizing Local Code Changes to Production Databases
Because resources are in the Postgres database, modifying files inside your local workspace's `.gemini/skills` or `.claude/skills` directory will not automatically update a remote server. You can synchronize skill changes bidirectionally:

#### CLI Client Synchronizer
From your workstation or a deploy script, run the Node.js helper to sync local skills to/from the remote server:
```bash
# Sync local markdown files to the remote Postgres database
API_KEY=<your-workspace-api-key> node scripts/memoryops-client.js sync-skills
```

#### VS Code Extension Sync
The VS Code extension includes the **Sync Agent Skills** command. This command:
1. Pulls all active skills from the remote Postgres instance.
2. Compares them to your local workspace files under `.gemini/skills/` and `.claude/skills/`.
3. Detects modifications, prompting you with version conflict resolutions (Push, Pull, or Merge) before modifying the database or local files.

---

## Complete Environment Variable Reference

These environment variables configure MemoryOps. They can be placed in a `.env` file in the working directory or injected into the container shell.

| Variable Name | Required | Default Value | Description |
|---|---|---|---|
| `DATABASE_URL` | **Yes** | `postgres://memoryops:memoryops@localhost:5432/memoryops` | Connection string for the PostgreSQL database. |
| `REDIS_URL` | **Yes** | `redis://localhost:6379` | Connection string for the Redis queue. |
| `QDRANT_URL` | **Yes** | `http://localhost:6334` | gRPC/HTTP URL for Qdrant vector database. |
| `CONFIG_PATH` | No | `config.toml` | Path to the TOML configuration file. |
| `APP_HOST` | No | `0.0.0.0` | Host IP address the API server binds to. |
| `APP_PORT` | No | `8080` | Port the API server listens on. |
| `APP_ENV` | No | `development` | Setting to `production` enforces strict secret key validation. |
| `APP_SECRET_KEY` | **Yes** (in production) | — | Cryptographic key used to encrypt skill credentials. Must be stable across restarts. |
| `WORKSPACE_CREATION_SECRET` | Required only when workspace creation is enabled | — | Secret token required to authenticate workspace creation requests (`x-admin-token`). Rotate or remove after bootstrap. |
| `WORKSPACE_CREATION_ENABLED` | No | `true` in local compose, `false` in production overlay | Set to `false` after initial bootstrap to disable `POST /v1/workspaces`. |
| `SKIP_MIGRATIONS` | No | `false` | When set to `true`, bypasses database migrations on API server startup. |
| `TRUSTED_PROXY_CIDRS` | No | `127.0.0.1/32` | Comma-separated CIDR blocks representing trusted reverse proxies. |

For the complete production hardening checklist, see [security-production.md](security-production.md).
| `MEMORYOPS_ALLOW_PRIVATE_IPS` | No | `false` | Set to `true` to allow skills to target internal/loopback IP addresses. |
| `QDRANT_CHECK_COMPATIBILITY` | No | `false` | Bypasses major/minor version verification between client library and Qdrant database. |
| `MCP_TRANSPORT` | No | `stdio` | Transport type for the MCP server (`stdio` or `http`). |
| `MCP_PORT` | No | `3003` | Port for the MCP server when transport is `http`. |
| `RUST_LOG` | No | `info` | Logging framework level filtering (`trace`, `debug`, `info`, `warn`, `error`). |
| `OPENAI_API_KEY` | Conditional | — | Required if using OpenAI LLM or OpenAI embedding providers. |
| `ANTHROPIC_API_KEY` | Conditional | — | Required if using Anthropic LLM provider. |
| `GEMINI_API_KEY` | Conditional | — | Required if using Google Gemini LLM provider. |
| `OPENROUTER_API_KEY` | Conditional | — | Required if using OpenRouter LLM provider. |
| `HF_API_KEY` | Conditional | — | Required if using Hugging Face Inference Router LLM provider. |
| `MEMORYOPS_WORKSPACE_ID` | No | — | Runtime Workspace ID injected into the Frontend. |
