import { Activity, AlertCircle, AlertTriangle, BookOpen, CheckCircle2, Check, X, Search, Keyboard } from "lucide-react";
import { useQuery } from "@tanstack/react-query";
import { useState, useMemo, useEffect } from "react";

import { CodeBlock } from "../components/CodeBlock";
import { getSystemHealth, type SystemHealthResponse } from "../api/health";
import { listIntegrations } from "../api/integrations";
import { HelpTooltip, Tooltip, TooltipContent, TooltipTrigger } from "../components/ui/tooltip";
import { cn } from "../lib/utils";
import { useAppStore } from "../store/app-store";

const SECTIONS = [
  { id: "overview", label: "Overview" },
  { id: "authentication", label: "Authentication" },
  { id: "vscode", label: "VS Code Extension" },
  { id: "claude-desktop", label: "Claude Desktop" },
  { id: "agent-observations", label: "Agent Observations" },
  { id: "openwebui", label: "OpenWebUI" },
  { id: "direct-api", label: "Direct API" },
  { id: "ingest", label: "Ingest Memories" },
  { id: "retrieve", label: "Search & Retrieve" },
  { id: "skills", label: "Skills" },
  { id: "contradictions", label: "Contradictions" },
  { id: "lifecycle", label: "Lifecycle & Decay" },
  { id: "config", label: "Workspace Config" },
  { id: "export-import", label: "Export & Import" },
  { id: "troubleshooting", label: "Troubleshooting" },
] as const;

export function GuideView() {
  const workspaceId = useAppStore((s: any) => s.workspaceId);
  const apiKey = useAppStore((s: any) => s.apiKey);
  const hasAuth = workspaceId.trim().length > 0 && apiKey.trim().length > 0;
  const [searchQuery, setSearchQuery] = useState("");
  const [searchResults, setSearchResults] = useState<(typeof SECTIONS[number])[]>([]);
  const [showSearchResults, setShowSearchResults] = useState(false);

  const healthQuery = useQuery({
    queryKey: ["system-health"],
    queryFn: getSystemHealth,
    refetchInterval: 30_000,
    enabled: hasAuth,
  });

  const integrationsQuery = useQuery({
    queryKey: ["integrations", workspaceId],
    queryFn: () => listIntegrations(workspaceId),
    enabled: hasAuth,
    refetchInterval: 60_000,
  });

  // Search functionality
  const filteredSections = useMemo(() => {
    if (!searchQuery.trim()) return SECTIONS;
    const query = searchQuery.toLowerCase();
    return SECTIONS.filter((section) =>
      section.label.toLowerCase().includes(query)
    );
  }, [searchQuery]);

  useEffect(() => {
    if (searchQuery.trim()) {
      setSearchResults([...filteredSections]);
      setShowSearchResults(true);
    } else {
      setSearchResults([]);
      setShowSearchResults(false);
    }
  }, [searchQuery, filteredSections]);

  // Integration status helper
  const getIntegrationStatus = (source: string) => {
    if (!integrationsQuery.data) return { status: "unknown", configured: false, integration: undefined as undefined };
    const integration = integrationsQuery.data.find((i: any) => i.source === source);
    if (!integration) return { status: "not-configured", configured: false, integration: undefined as undefined };
    if (integration.status === "active" && integration.errors_24h === 0) {
      return { status: "healthy", configured: true, integration };
    }
    if (integration.status === "active" && integration.errors_24h > 0) {
      return { status: "degraded", configured: true, integration };
    }
    return { status: "failing", configured: true, integration };
  };

  return (
    <div className="mx-auto max-w-7xl">
      <header className="mb-6">
        <p className="text-sm font-medium text-accent-strong">Documentation</p>
        <h1 className="mt-1 text-2xl font-semibold tracking-normal text-ink">Connection Guide</h1>
      </header>

      {hasAuth ? <HealthStrip health={healthQuery.data} loading={healthQuery.isLoading} /> : null}

      {/* Search Bar */}
      <div className="mb-6 relative">
        <div className="relative">
          <Search className="absolute left-3 top-1/2 h-4 w-4 -translate-y-1/2 text-ink/40" aria-hidden="true" />
          <input
            type="text"
            placeholder="Search guide sections..."
            value={searchQuery}
            onChange={(e) => setSearchQuery(e.target.value)}
            onFocus={() => setShowSearchResults(searchQuery.trim().length > 0)}
            onBlur={() => setTimeout(() => setShowSearchResults(false), 200)}
            className="w-full rounded-lg border border-line bg-white px-10 py-2.5 text-sm text-ink placeholder:text-ink/40 focus:border-accent focus:outline-none focus:ring-2 focus:ring-accent/20"
          />
          <div className="absolute right-3 top-1/2 flex -translate-y-1/2 items-center gap-1 text-xs text-ink/40">
            <Keyboard className="h-3 w-3" aria-hidden="true" />
            <span>/</span>
          </div>
        </div>

        {/* Search Results Dropdown */}
        {showSearchResults && searchResults.length > 0 && (
          <div className="absolute z-10 mt-2 w-full rounded-lg border border-line bg-white shadow-lg">
            <ul className="max-h-64 overflow-y-auto py-1">
              {searchResults.map((section) => (
                <li key={section.id}>
                  <a
                    href={`#${section.id}`}
                    onClick={() => setSearchQuery("")}
                    className="block px-4 py-2 text-sm text-ink/70 hover:bg-soft hover:text-ink"
                  >
                    {section.label}
                  </a>
                </li>
              ))}
            </ul>
          </div>
        )}
      </div>

      <div className="mt-6 grid gap-8 lg:grid-cols-[220px_1fr]">
        <aside className="hidden lg:block">
          <nav className="sticky top-8 rounded-lg border border-line bg-white p-3" aria-label="Guide sections">
            <p className="mb-2 px-2 text-xs font-semibold uppercase tracking-wider text-ink/45">Contents</p>
            <ul className="grid gap-0.5">
              {SECTIONS.map((section) => (
                <li key={section.id}>
                  <a
                    href={`#${section.id}`}
                    className="block rounded px-2 py-1.5 text-sm text-ink/70 transition hover:bg-soft hover:text-ink"
                  >
                    {section.label}
                  </a>
                </li>
              ))}
            </ul>
          </nav>
        </aside>

        <div className="grid gap-12 min-w-0">
          <Section id="overview" title="Overview">
            <p>
              MemoryOps is a self-hosted memory layer for AI agents. It stores, retrieves, and manages memories produced
              by language models, giving every agent a persistent and searchable knowledge base.
            </p>
            <p className="mt-3">
              Connect your AI tools — VS Code/Copilot, Claude Desktop, OpenWebUI, or any HTTP client — using a workspace
              ID and API key. All data stays on your infrastructure.
            </p>
            <div className="my-6 grid gap-4 md:grid-cols-2">
              <div className="rounded-lg border border-line bg-soft/50 p-4">
                <h3 className="font-semibold text-ink flex items-center gap-2">
                  <span className="h-2 w-2 rounded-full bg-blue-500" />
                  Episodic Memory
                </h3>
                <p className="mt-2 text-xs text-ink/75 leading-relaxed">
                  Discrete, point-in-time experiences or events (e.g., git commits, Slack messages, deployment logs, or agent observations). These are highly time-sensitive, subject to mathematical decay, and act as raw inputs to the memory engine.
                </p>
              </div>
              <div className="rounded-lg border border-line bg-soft/50 p-4">
                <h3 className="font-semibold text-ink flex items-center gap-2">
                  <span className="h-2 w-2 rounded-full bg-emerald-500" />
                  Semantic Memory
                </h3>
                <p className="mt-2 text-xs text-ink/75 leading-relaxed">
                  Durable, consolidated facts, rules, and workspace concepts (e.g., architectural guidelines or database connection limits). Version-controlled, immune to decay, and created manually or via the Promotion Pipeline (which clusters related episodic memories).
                </p>
              </div>
            </div>
            <Callout>
              You will need a <strong>Workspace ID</strong> <HelpTooltip label="Workspace ID">Workspace boundary that MemoryOps uses to isolate memories, settings, and retrieval scope.</HelpTooltip> and an <strong>API Key</strong> <HelpTooltip label="API Key">Credential used by clients and tools to authenticate against the selected MemoryOps workspace.</HelpTooltip>. Set them in{" "}
              <a href="/settings" className="text-accent-strong underline underline-offset-2">Settings</a> to have them
              auto-filled in the code examples below.
            </Callout>
          </Section>

          <Section id="authentication" title="Authentication">
            <p>
              Every request must include your API key in the <code className="inline-code">x-api-key</code> header.
              Create a key from the Settings page after creating a workspace.
            </p>
            <CodeBlock code={`curl {{API_URL}}/v1/workspaces/{{WORKSPACE_ID}} \\
  -H "x-api-key: {{API_KEY}}"`} />
            <p className="mt-4">
              The API returns standard HTTP status codes. <code className="inline-code">401</code> means the key is
              missing or invalid. <code className="inline-code">403</code> means the key does not have access to the
              requested workspace.
            </p>
          </Section>

          <Section id="vscode" title="VS Code Extension — Coming Soon" status={null}>
            <p>
              A first-party <strong>MemoryOps VS Code extension</strong> is planned. The goal is to bring workspace
              memory directly into the editor so coding agents and developers can retrieve relevant project context
              without leaving VS Code.
            </p>
            <Callout>
              The extension is not published yet. For now, connect editor-based agents through the MCP server or call
              the REST API directly using the examples in this guide.
            </Callout>
            <p className="mt-2 font-medium text-ink">Planned capabilities</p>
            <ul className="mt-2 grid gap-2 pl-4 text-ink/80" style={{ listStyleType: "disc" }}>
              <li>Configure <code className="inline-code">memoryops.apiUrl</code>, <code className="inline-code">memoryops.workspaceId</code>, and <code className="inline-code">memoryops.apiKey</code> from VS Code settings.</li>
              <li>Search and retrieve MemoryOps context from the Command Palette.</li>
              <li>Save highlighted code, notes, or decisions as agent observations.</li>
              <li>Surface relevant memories while working in a repository, pull request, or incident/debugging context.</li>
              <li>Provide a bridge for Copilot-style workflows to use governed MemoryOps context.</li>
            </ul>
            <CodeBlock code={`// planned settings.json shape
{
  "memoryops.apiUrl": "{{API_URL}}",
  "memoryops.workspaceId": "{{WORKSPACE_ID}}",
  "memoryops.apiKey": "{{API_KEY}}",
  "memoryops.defaultTopK": 5
}`} />
          </Section>

          <Section id="claude-desktop" title="Claude Desktop (MCP)" tooltip="MCP lets MemoryOps expose memory read and write tools directly to compatible agent clients." status={null}>
            <p>
              MemoryOps exposes an MCP server so Claude Desktop can read and write memories automatically during
              conversations. Add the server to your Claude Desktop configuration file.
            </p>
            <p className="mt-2 text-sm text-ink/70">
              Configuration file location: <code className="inline-code">~/.config/claude/claude_desktop_config.json</code>{" "}
              (macOS/Linux) or <code className="inline-code">%APPDATA%\Claude\claude_desktop_config.json</code> (Windows).
            </p>
            <CodeBlock code={`{
  "mcpServers": {
    "memoryops": {
      "command": "memoryops-mcp",
      "env": {
        "MEMORYOPS_API_URL": "{{API_URL}}",
        "MEMORYOPS_WORKSPACE_ID": "{{WORKSPACE_ID}}",
        "MEMORYOPS_API_KEY": "{{API_KEY}}"
      }
    }
  }
}`} />
            <Callout>
              The <code className="inline-code">memoryops-mcp</code> binary is included in the MemoryOps release
              archive. Place it on your <code className="inline-code">PATH</code> before starting Claude Desktop.
            </Callout>
          </Section>

          <Section id="agent-observations" title="Agent Observations" tooltip="First-party agent-submitted memories sent directly into the MemoryOps ingest pipeline.">
            <p>
              Agent Observations let any process — a CI pipeline, a background agent, a CLI script — push structured
              observations directly into MemoryOps without going through a webhook integration. Use observations when
              you control the sender and want scoped, importance-tagged memories tied to a specific agent.
            </p>
            <p className="mt-2">
              Use <strong>webhooks</strong> for third-party platforms (GitHub, Slack, Jira) that send events to a fixed
              URL. Use <strong>observations</strong> for first-party agents and automation that can authenticate with an
              API key and target a specific <code className="inline-code">agent_id</code>.
            </p>
            <CodeBlock code={`curl -X POST {{API_URL}}/v1/ingest/observation \\
  -H "x-api-key: {{API_KEY}}" \\
  -H "Content-Type: application/json" \\
  -d '{
    "workspace_id": "{{WORKSPACE_ID}}",
    "content": "Deployment pipeline completed in 142 s. All health checks passed.",
    "agent_id": "deploy-bot",
    "user_id": "quinn",
    "repo": "org/backend",
    "tags": ["deploy", "ci"],
    "importance": 0.75,
    "source_ref": "run/8821"
  }'`} />
            <p className="mt-2">
              The endpoint returns <code className="inline-code">202 Accepted</code> with the raw event id. The memory
              is embedded and indexed asynchronously by the processor.
            </p>
            <CodeBlock code={`{ "id": "<uuid>", "status": "queued" }`} />
            <p className="mt-2">
              The MCP <code className="inline-code">memory_store</code> tool routes through the same pipeline. Provide
              the optional fields to attach scope metadata:
            </p>
            <CodeBlock code={`// MCP tool call — memory_store
{
  "content": "User prefers concise answers with no trailing summaries.",
  "agent_id": "claude-code",
  "user_id": "quinn",
  "tags": ["preference", "output"],
  "importance": 0.8
}`} />
            <Callout>
              Idempotency is enforced per <code className="inline-code">(workspace_id, agent_id, content)</code>. Sending
              the same observation twice returns the existing event id without creating a duplicate.
            </Callout>
            <p className="mt-2">
              View recent observations in the{" "}
              <a href="/integrations" className="text-accent-strong underline underline-offset-2">Integrations</a>{" "}
              dashboard under the <strong>Observations</strong> tab.
            </p>
          </Section>

          <Section id="openwebui" title="OpenWebUI" status={null}>
            <p>
              Add MemoryOps as an OpenWebUI function to enrich every chat message with memories retrieved from your
              workspace.
            </p>
            <ol className="mt-3 grid gap-2 pl-4 text-ink/80" style={{ listStyleType: "decimal" }}>
              <li>In OpenWebUI, navigate to <strong>Workspace → Functions</strong>.</li>
              <li>Click <strong>+</strong> and paste the MemoryOps function code.</li>
              <li>Set the <code className="inline-code">MEMORYOPS_API_URL</code>, <code className="inline-code">WORKSPACE_ID</code>, and <code className="inline-code">API_KEY</code> valves.</li>
              <li>Enable the function for the models you want.</li>
            </ol>
            <CodeBlock code={`# Function valves
MEMORYOPS_API_URL = "{{API_URL}}"
MEMORYOPS_WORKSPACE_ID = "{{WORKSPACE_ID}}"
MEMORYOPS_API_KEY = "{{API_KEY}}"
TOP_K = 5`} />
          </Section>

          <Section id="direct-api" title="Direct REST API">
            <p>
              Any HTTP client can talk to MemoryOps directly. The base URL for all endpoints is{" "}
              <code className="inline-code">{"{{API_URL}}"}</code>.
            </p>
            <CodeBlock code={`# List workspaces
curl {{API_URL}}/v1/workspaces/{{WORKSPACE_ID}} \\
  -H "x-api-key: {{API_KEY}}"

# Query memories
curl -X POST {{API_URL}}/v1/workspaces/{{WORKSPACE_ID}}/query \\
  -H "x-api-key: {{API_KEY}}" \\
  -H "Content-Type: application/json" \\
  -d '{"query": "deployment process", "top_k": 5}'`} />
          </Section>

          <Section id="ingest" title="Ingest Memories">
            <p>
              Send raw text to be processed and stored as memories. MemoryOps splits long documents, extracts entities,
              embeds chunks, and detects contradictions automatically.
            </p>
            <CodeBlock code={`curl -X POST {{API_URL}}/v1/workspaces/{{WORKSPACE_ID}}/memories \\
  -H "x-api-key: {{API_KEY}}" \\
  -H "Content-Type: application/json" \\
  -d '{
    "content": "We deploy every Tuesday at 10 AM UTC via the release pipeline.",
    "source": "slack",
    "memory_type": "semantic",
    "tags": ["deploy", "process"]
  }'`} />
            <p className="mt-4">
              The response includes the new memory's <code className="inline-code">id</code>. Embedding and
              contradiction detection happen asynchronously in the background.
            </p>
            <div className="mt-6 border-t border-line pt-6">
              <h3 className="font-semibold text-ink text-sm">The MemoryOps Ingestion Pipeline</h3>
              <p className="mt-1 text-xs text-ink/70">
                The Ingestion Pipeline is the secure, high-throughput gateway of the MemoryOps control plane. It acts as an HMAC-validated receiver for developer workflow activity, translating raw tool actions into standardized memories.
              </p>
              <div className="mt-4 grid gap-4 md:grid-cols-3">
                <div className="rounded-lg border border-line bg-soft/30 p-3.5">
                  <h4 className="text-xs font-semibold text-ink flex items-center gap-1.5">
                    <span className="h-4 w-4 shrink-0 rounded-full bg-accent/15 text-accent flex items-center justify-center text-[10px] font-bold">1</span>
                    HMAC Security
                  </h4>
                  <p className="mt-1.5 text-xs text-ink/75 leading-relaxed">
                    Incoming webhooks are verified via strict HMAC signature checks (e.g. <code>X-Hub-Signature-256</code> or <code>X-Slack-Signature</code>) using integration-specific secrets to prevent malicious probes.
                  </p>
                </div>
                <div className="rounded-lg border border-line bg-soft/30 p-3.5">
                  <h4 className="text-xs font-semibold text-ink flex items-center gap-1.5">
                    <span className="h-4 w-4 shrink-0 rounded-full bg-accent/15 text-accent flex items-center justify-center text-[10px] font-bold">2</span>
                    Atomic Idempotency
                  </h4>
                  <p className="mt-1.5 text-xs text-ink/75 leading-relaxed">
                    Duplicate webhook deliveries are rejected automatically. The ingestion transaction guarantees that a raw event is stored in PostgreSQL and enqueued in Redis Streams (<code>XADD</code>) atomically.
                  </p>
                </div>
                <div className="rounded-lg border border-line bg-soft/30 p-3.5">
                  <h4 className="text-xs font-semibold text-ink flex items-center gap-1.5">
                    <span className="h-4 w-4 shrink-0 rounded-full bg-accent/15 text-accent flex items-center justify-center text-[10px] font-bold">3</span>
                    Decoupled Processing
                  </h4>
                  <p className="mt-1.5 text-xs text-ink/75 leading-relaxed">
                    Webhooks immediately return <code>202 Accepted</code>. Asynchronous workers in the <code>processor</code> crate handle parsing, entity extraction, importance scoring, and vector indexing (Qdrant) in the background.
                  </p>
                </div>
              </div>
            </div>
          </Section>

          <Section id="retrieve" title="Search & Retrieve">
            <p>
              Query memories using natural language. The retrieval engine performs dense vector search and can filter by
              tag, source, or memory type.
            </p>
            <CodeBlock code={`curl -X POST {{API_URL}}/v1/workspaces/{{WORKSPACE_ID}}/query \\
  -H "x-api-key: {{API_KEY}}" \\
  -H "Content-Type: application/json" \\
  -d '{
    "query": "how do we handle on-call escalations?",
    "top_k": 8,
    "filters": {
      "memory_type": "semantic",
      "tags": ["on-call"]
    }
  }'`} />
          </Section>

          <Section id="skills" title="Skills" tooltip="HTTP tools agents can call to augment memory retrieval with live external data.">
            <p>
              Skills are HTTP endpoints that agents can call at retrieval time to augment answers with live data. Register
              a skill with a name, URL, and JSON schema, then enable it for the workspace.
            </p>
            <CodeBlock code={`curl -X POST {{API_URL}}/v1/workspaces/{{WORKSPACE_ID}}/skills \\
  -H "x-api-key: {{API_KEY}}" \\
  -H "Content-Type: application/json" \\
  -d '{
    "name": "weather",
    "description": "Returns current weather for a city",
    "endpoint_url": "https://api.example.com/weather",
    "http_method": "POST",
    "input_schema": {
      "type": "object",
      "properties": {
        "city": { "type": "string" }
      },
      "required": ["city"]
    },
    "enabled": true
  }'`} />
            <p className="mt-4">
              Use the <strong>Test</strong> button in the Skills table to send a live request and inspect the response
              without leaving the dashboard.
            </p>
          </Section>

          <Section id="contradictions" title="Contradictions" tooltip="Review queue for memories that may disagree and need operator resolution.">
            <p>
              When MemoryOps detects that two memories conflict, it creates a contradiction flag. You can review flags
              in the <a href="/contradictions" className="text-accent-strong underline underline-offset-2">Contradictions</a>{" "}
              view and choose an action.
            </p>
            <ul className="mt-3 grid gap-1.5 pl-4 text-ink/80" style={{ listStyleType: "disc" }}>
              <li><strong>Accept both</strong> — keep both memories as-is.</li>
              <li><strong>Dismiss</strong> — mark the flag as resolved without changing memories.</li>
              <li><strong>Keep A / Keep B</strong> — keep the selected memory and soft-delete the other.</li>
            </ul>
            <CodeBlock code={`# Resolve via API — keep memory A, discard memory B
curl -X POST {{API_URL}}/v1/workspaces/{{WORKSPACE_ID}}/contradictions/<flag_id>/resolve \\
  -H "x-api-key: {{API_KEY}}" \\
  -H "Content-Type: application/json" \\
  -d '{"resolution": "keep_a"}'`} />
          </Section>

          <Section id="lifecycle" title="Lifecycle & Decay" tooltip="Rules that age, prune, and promote memories over time.">
            <p>
              Memories decay over time based on a configurable half-life. Memories below the pruning threshold are
              promoted to long-term storage or archived. You can tune both values per workspace.
            </p>
            <CodeBlock code={`# Update decay settings
curl -X PATCH {{API_URL}}/v1/workspaces/{{WORKSPACE_ID}}/config \\
  -H "x-api-key: {{API_KEY}}" \\
  -H "Content-Type: application/json" \\
  -d '{
    "decay_half_life_days": 30,
    "pruning_threshold": 0.1
  }'`} />
            <p className="mt-4">
              Trigger a manual promotion cycle from the <a href="/settings" className="text-accent-strong underline underline-offset-2">Settings</a>{" "}
              page to apply the latest lifecycle rules immediately.
            </p>
          </Section>

          <Section id="config" title="Workspace Configuration">
            <p>
              Patch any workspace setting with a <code className="inline-code">PATCH /config</code> request. Only fields
              included in the request body are updated.
            </p>
            <CodeBlock code={`curl -X PATCH {{API_URL}}/v1/workspaces/{{WORKSPACE_ID}}/config \\
  -H "x-api-key: {{API_KEY}}" \\
  -H "Content-Type: application/json" \\
  -d '{
    "llm_provider": "openai",
    "llm_model": "gpt-4o-mini",
    "embedding_provider": "openai",
    "embedding_model": "text-embedding-3-small",
    "contradiction_mode": "quarantine"
  }'`} />
          </Section>

          <Section id="export-import" title="Export & Import" tooltip="Workspace backup and restore flows for memory migration or recovery.">
            <p>
              Back up all memories as newline-delimited JSON (NDJSON), or restore them from a previous export. Useful
              for migrating between workspaces or keeping an offline archive.
            </p>
            <CodeBlock code={`# Export
curl {{API_URL}}/v1/workspaces/{{WORKSPACE_ID}}/export \\
  -H "x-api-key: {{API_KEY}}" \\
  -o memories.ndjson

# Import
curl -X POST {{API_URL}}/v1/workspaces/{{WORKSPACE_ID}}/import \\
  -H "x-api-key: {{API_KEY}}" \\
  -H "Content-Type: application/x-ndjson" \\
  --data-binary @memories.ndjson`} />
          </Section>

          <Section id="troubleshooting" title="Troubleshooting">
            <p>
              Use the system health endpoint to check database, cache, and vector-store connectivity. The dashboard
              also shows a live health strip at the top of this page when you are authenticated.
            </p>
            <CodeBlock code={`curl {{API_URL}}/health/system \\
  -H "x-api-key: {{API_KEY}}"

# Expected response
{
  "status": "healthy",
  "checks": [
    { "name": "postgres", "status": "ok", "latency_ms": 2 },
    { "name": "redis",    "status": "ok", "latency_ms": 1 },
    { "name": "qdrant",   "status": "ok", "latency_ms": 5 }
  ]
}`} />
            <p className="mt-4 font-medium text-ink">Common issues</p>
            <ul className="mt-2 grid gap-2 pl-4 text-sm text-ink/80" style={{ listStyleType: "disc" }}>
              <li><strong>401 Unauthorized</strong> — Check that the <code className="inline-code">x-api-key</code> header is present and correct. Regenerate keys from Settings if needed.</li>
              <li><strong>Authentication failures</strong> — Verify your API key is valid for the workspace. Check the key hasn't been revoked or expired.</li>
              <li><strong>Webhook signature mismatches</strong> — Ensure webhook secrets match between MemoryOps and the external service (GitHub, Slack, Jira, Linear). Check for trailing whitespace or encoding issues.</li>
              <li><strong>Embeddings not updating</strong> — Verify the processor worker is running and Redis is reachable. Check the DLQ for failed embedding jobs.</li>
              <li><strong>No search results</strong> — Trigger a re-index from Settings to rebuild the vector index. Ensure memories have been ingested and embeddings generated.</li>
              <li><strong>MCP connection timeouts</strong> — Check the MCP server is running on the correct port (3003 for HTTP). Verify network connectivity and firewall settings.</li>
              <li><strong>MCP not connecting</strong> — Ensure <code className="inline-code">memoryops-mcp</code> is on your PATH and the env vars are set correctly. Check stdio transport requires cargo in PATH.</li>
              <li><strong>Rate limiting errors (429)</strong> — You've exceeded the rate limit for your workspace. Wait and retry, or increase limits in workspace config if needed.</li>
            </ul>
          </Section>
        </div>
      </div>
    </div>
  );
}

type IntegrationStatus = "active" | "degraded" | "failing" | string;

type IntegrationData = {
  source: string;
  last_event_at?: string | null;
  events_24h: number;
  errors_24h: number;
  status: IntegrationStatus;
};

function Section({ id, title, tooltip, status, children }: { id: string; title: string; tooltip?: string; status?: { status: string; configured: boolean; integration: IntegrationData | undefined } | null; children: React.ReactNode }) {
  return (
    <section id={id} className="scroll-mt-6">
      <h2 className="mb-4 inline-flex items-center gap-2 text-xl font-semibold text-ink">
        <span>{title}</span>
        {tooltip ? <HelpTooltip label={title}>{tooltip}</HelpTooltip> : null}
        {status && <IntegrationStatusIndicator status={status.status} configured={status.configured} integration={status.integration} />}
      </h2>
      <div className="prose-like grid gap-3 text-sm leading-relaxed text-ink/80">{children}</div>
    </section>
  );
}

function IntegrationStatusIndicator({ status, configured, integration }: { status: string; configured: boolean; integration: IntegrationData | undefined }) {
  if (status === "unknown" || !configured) {
    return (
      <Tooltip>
        <TooltipTrigger asChild>
          <span className="flex items-center gap-1 rounded-full bg-gray-100 px-2 py-0.5 text-xs text-gray-600 cursor-help">
            <X className="h-3 w-3" aria-hidden="true" />
            Not configured
          </span>
        </TooltipTrigger>
        <TooltipContent>This integration is not configured yet.</TooltipContent>
      </Tooltip>
    );
  }

  if (status === "healthy") {
    return (
      <Tooltip>
        <TooltipTrigger asChild>
          <span className="flex items-center gap-1 rounded-full bg-green-100 px-2 py-0.5 text-xs text-green-700 cursor-help">
            <Check className="h-3 w-3" aria-hidden="true" />
            Healthy
          </span>
        </TooltipTrigger>
        <TooltipContent>
          {integration ? `Last event: ${integration.last_event_at ? new Date(integration.last_event_at).toLocaleString() : 'Never'} • ${integration.events_24h} events in 24h` : 'Configured and healthy'}
        </TooltipContent>
      </Tooltip>
    );
  }

  if (status === "degraded") {
    return (
      <Tooltip>
        <TooltipTrigger asChild>
          <span className="flex items-center gap-1 rounded-full bg-amber-100 px-2 py-0.5 text-xs text-amber-700 cursor-help">
            <AlertTriangle className="h-3 w-3" aria-hidden="true" />
            Degraded
          </span>
        </TooltipTrigger>
        <TooltipContent>
          {integration ? `${integration.errors_24h} errors in 24h • Check DLQ for details` : 'Partially configured with errors'}
        </TooltipContent>
      </Tooltip>
    );
  }

  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <span className="flex items-center gap-1 rounded-full bg-red-100 px-2 py-0.5 text-xs text-red-700 cursor-help">
          <X className="h-3 w-3" aria-hidden="true" />
          Failing
        </span>
      </TooltipTrigger>
      <TooltipContent>
        {integration ? 'Integration is failing • Check DLQ and logs' : 'Configuration error'}
      </TooltipContent>
    </Tooltip>
  );
}

function Callout({ children }: { children: React.ReactNode }) {
  return (
    <div className="flex gap-3 rounded-lg border border-blue-200 bg-blue-50 px-4 py-3 text-sm text-blue-800">
      <BookOpen className="mt-0.5 h-4 w-4 shrink-0 text-blue-500" aria-hidden="true" />
      <span>{children}</span>
    </div>
  );
}

function HealthStrip({ health, loading }: { health: SystemHealthResponse | undefined; loading: boolean }) {
  if (loading) {
    return (
      <div className="flex items-center gap-2 rounded-lg border border-line bg-white px-4 py-2.5 text-sm text-ink/60">
        <Activity className="h-4 w-4 animate-pulse" aria-hidden="true" />
        Checking system health…
      </div>
    );
  }

  if (!health) {
    return null;
  }

  const overall = health.status;
  const allOk = overall === "healthy";

  return (
    <div
      className={cn(
        "flex flex-wrap items-center gap-x-5 gap-y-2 rounded-lg border px-4 py-2.5 text-sm",
        allOk
          ? "border-green-200 bg-green-50 text-green-800"
          : overall === "degraded"
            ? "border-amber-200 bg-amber-50 text-amber-800"
            : "border-red-200 bg-red-50 text-red-800",
      )}
      role="status"
      aria-label={`System status: ${overall}`}
    >
      <span className="flex items-center gap-1.5 font-medium">
        {allOk
          ? <CheckCircle2 className="h-4 w-4" aria-hidden="true" />
          : <AlertCircle className="h-4 w-4" aria-hidden="true" />}
        System {overall}
        <HelpTooltip label="Health strip">Live view of the backend services MemoryOps depends on while you work through setup and troubleshooting.</HelpTooltip>
      </span>
      {health.checks.map((check) => (
        <Tooltip key={check.name}>
          <TooltipTrigger asChild>
            <span tabIndex={0} className="flex items-center gap-1 rounded-sm text-xs opacity-80 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent">
              <span
                className={cn(
                  "h-2 w-2 rounded-full",
                  check.status === "ok" ? "bg-green-500" : check.status === "warn" ? "bg-amber-400" : "bg-red-500",
                )}
                aria-hidden="true"
              />
              {check.name}
              {check.latency_ms !== null ? ` ${check.latency_ms}ms` : ""}
            </span>
          </TooltipTrigger>
          <TooltipContent>{check.message ?? `${check.name} is reporting ${check.status}.`}</TooltipContent>
        </Tooltip>
      ))}
    </div>
  );
}
