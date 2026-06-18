import { Activity, AlertCircle, AlertTriangle, BookOpen, CheckCircle2, Check, X, Search, Keyboard } from "lucide-react";
import { useState, useMemo, useEffect } from "react";

import { CodeBlock } from "../components/CodeBlock";
import type { SystemHealthResponse } from "../api/health";
import { HelpTooltip, Tooltip, TooltipContent, TooltipTrigger } from "../components/ui/tooltip";
import { useSystemHealth, useWorkspaceIntegrations } from "../hooks/use-live-query";
import { cn } from "../lib/utils";
import { useAppStore } from "../store/app-store";

type GuideSection = {
  id: string;
  label: string;
  keywords: string;
};

const SECTIONS = [
  { id: "overview", label: "Overview", keywords: "what is memoryops self hosted memory control plane agents ai" },
  { id: "mental-model", label: "Mental Model", keywords: "workspace memory unit episodic semantic scope tags source event embeddings trace lifecycle" },
  { id: "app-map", label: "App Map", keywords: "sidebar navigation pages dashboard memory traces lifecycle ingest integrations tools skills contradictions audit settings" },
  { id: "dashboard", label: "Dashboard", keywords: "metrics stats health trend quick jumps activity breakdown" },
  { id: "memory-explorer", label: "Memory Explorer", keywords: "browse filter search list memories tags source sort pinned importance deleted" },
  { id: "memory-detail", label: "Memory Detail", keywords: "edit restore promote publish merge provenance history feedback metadata" },
  { id: "retrieval-trace", label: "Retrieval Trace", keywords: "retrieve query token pack score breakdown candidate included excluded trace id" },
  { id: "ingest", label: "Ingest Memories", keywords: "manual create webhook github slack jira linear observation processor queue" },
  { id: "integrations", label: "Integrations", keywords: "webhook secret hmac dead letter queue dlq observation retry discard health" },
  { id: "tools", label: "Tools", keywords: "tool registry http endpoint schema secret invocation versions rollback test" },
  { id: "agent-skills", label: "Agent Library", keywords: "claude gemini instructions prompts skills markdown agent behavior" },
  { id: "contradictions", label: "Contradictions", keywords: "conflict review accept both dismiss keep a keep b resolution quarantine" },
  { id: "lifecycle", label: "Lifecycle & Decay", keywords: "decay half life pruning promotion semantic archive restore publish merge" },
  { id: "audit", label: "Audit", keywords: "operator history change log compliance key lifecycle events" },
  { id: "settings", label: "Settings", keywords: "api url key workspace provider model llm embedding reindex promotion config" },
  { id: "authentication", label: "Authentication", keywords: "x api key workspace auth 401 403 workspace creation secret" },
  { id: "vscode", label: "VS Code Extension", keywords: "editor copilot command palette extension settings repository context" },
  { id: "claude-desktop", label: "Claude Desktop", keywords: "mcp stdio claude config memory store retrieve desktop" },
  { id: "agent-observations", label: "Agent Observations", keywords: "first party agents ci deploy bot observation endpoint idempotency" },
  { id: "openwebui", label: "OpenWebUI", keywords: "function valves chat memory enrichment top k" },
  { id: "direct-api", label: "Direct API", keywords: "rest endpoints curl memory search retrieve trace workspace export import" },
  { id: "backend-architecture", label: "Backend Architecture", keywords: "rust axum postgres redis qdrant processor scheduler workers llm embeddings" },
  { id: "export-import", label: "Export & Import", keywords: "backup restore ndjson migrate workspace archive" },
  { id: "troubleshooting", label: "Troubleshooting", keywords: "health ready system errors no search results embeddings mcp rate limit" },
] as const satisfies readonly GuideSection[];

export function GuideView() {
  const workspaceId = useAppStore((s: any) => s.workspaceId);
  const apiKey = useAppStore((s: any) => s.apiKey);
  const hasAuth = workspaceId.trim().length > 0 && apiKey.trim().length > 0;
  const [searchQuery, setSearchQuery] = useState("");
  const [searchResults, setSearchResults] = useState<(typeof SECTIONS[number])[]>([]);
  const [showSearchResults, setShowSearchResults] = useState(false);

  const healthQuery = useSystemHealth(hasAuth);
  const integrationsQuery = useWorkspaceIntegrations(workspaceId, hasAuth);

  const filteredSections = useMemo(() => {
    if (!searchQuery.trim()) return SECTIONS;
    const query = searchQuery.toLowerCase();
    return SECTIONS.filter((section) =>
      section.label.toLowerCase().includes(query) || section.keywords.toLowerCase().includes(query),
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
        <h1 className="mt-1 text-2xl font-semibold tracking-normal text-ink">Connection & App Guide</h1>
        <p className="mt-2 max-w-3xl text-sm leading-relaxed text-ink/70">
          Use this guide as the operator manual for MemoryOps. It explains what each page does, how data moves through the
          platform, which backend endpoint powers each workflow, and how agents should connect to the memory layer.
        </p>
      </header>

      {hasAuth ? <HealthStrip health={healthQuery.data} loading={healthQuery.isLoading} /> : null}

      <div className="mb-6 relative">
        <div className="relative">
          <Search className="absolute left-3 top-1/2 h-4 w-4 -translate-y-1/2 text-ink/40" aria-hidden="true" />
          <input
            type="text"
            placeholder="Search guide sections, concepts, pages, or endpoints..."
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

      <div className="mt-6 grid gap-8 lg:grid-cols-[240px_1fr]">
        <aside className="hidden lg:block">
          <nav className="sticky top-8 max-h-[calc(100vh-4rem)] overflow-y-auto rounded-lg border border-line bg-white p-3" aria-label="Guide sections">
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
              MemoryOps is a self-hosted memory control plane for AI agents. It turns raw activity from humans, agents,
              repositories, chats, incidents, and automation into governed memories that can be searched, retrieved,
              promoted, corrected, audited, and reused across tools.
            </p>
            <p>
              The app is intentionally split into two jobs: the <strong>backend</strong> owns durable memory operations,
              retrieval, lifecycle, and ingestion; the <strong>frontend Control Center</strong> gives operators a safe place to
              inspect, tune, and correct the memory layer without writing curl commands for every action.
            </p>
            <div className="my-6 grid gap-4 md:grid-cols-2">
              <MiniCard title="Episodic Memory" accent="blue">
                Point-in-time activity such as commits, PR comments, Slack messages, ticket updates, deployment logs, and
                agent observations. Episodic memories are useful for recent context and are subject to decay.
              </MiniCard>
              <MiniCard title="Semantic Memory" accent="green">
                Durable knowledge such as architectural decisions, team rules, project facts, and stable preferences.
                Semantic memories should survive longer and are the ideal material for future agents.
              </MiniCard>
            </div>
            <Callout>
              The normal operator loop is: <strong>ingest</strong> events, <strong>retrieve</strong> context, inspect the
              <strong> trace</strong>, resolve contradictions, promote durable knowledge, and audit what changed.
            </Callout>
          </Section>

          <Section id="mental-model" title="Mental Model" tooltip="Core nouns used throughout the UI and API.">
            <p>
              MemoryOps is easier to understand when every screen is mapped to the same set of underlying objects.
            </p>
            <DefinitionGrid
              items={[
                ["Workspace", "Isolation boundary for memories, API keys, integrations, provider settings, lifecycle rules, and audit events."],
                ["Memory unit", "The canonical stored item. It has content, type, scope, importance, tags, timestamps, source references, and retrieval metadata."],
                ["Scope", "Optional agent_id, user_id, and repo values that control which private or shared memory layers are eligible during retrieval."],
                ["Source event", "A raw incoming webhook or observation before processing. Source events provide provenance for later debugging."],
                ["Embedding", "Vector representation used for semantic search in Qdrant. If embeddings are missing, retrieval quality drops."],
                ["Retrieval trace", "A persisted explanation of which candidates were considered, scored, included, or excluded for a query."],
                ["Lifecycle", "Decay, promotion, pruning, restore, publish, merge, and feedback operations that govern memory quality over time."],
                ["Contradiction", "A detected conflict between memories that needs an operator or policy decision."],
              ]}
            />
            <p>
              Most UI actions are thin wrappers around REST endpoints. The UI adds guardrails, previews, pagination,
              optimistic updates, and safer copy, but the backend remains the source of truth.
            </p>
          </Section>

          <Section id="app-map" title="App Map">
            <p>
              Use the sidebar as a workflow map. Start on the Dashboard, drill into Memory or Traces when something looks
              off, and use Lifecycle, Contradictions, Integrations, Tools, and Settings to govern the system.
            </p>
            <PageWalkthrough
              title="Dashboard"
              href="/"
              purpose="Workspace command center for health, memory volume, trend, contradiction count, and quick jumps."
              reads="Readiness, workspace stats, stats history, and contradiction count."
              actions="Navigate into deeper workflows."
              how="It periodically reads aggregate backend state and summarizes whether the memory pool is healthy, stale, noisy, or growing."
            />
            <PageWalkthrough
              title="Memory"
              href="/memory"
              purpose="Search, filter, and inspect the actual memories stored in the workspace."
              reads="/v1/memory and /v1/memory/search."
              actions="Open detail pages, filter by type/scope/source/tags, and find recoverable deleted items."
              how="It queries PostgreSQL for metadata and uses backend search endpoints when a natural-language query is supplied."
            />
            <PageWalkthrough
              title="Traces"
              href="/trace"
              purpose="Explain why a query returned a particular context pack."
              reads="/v1/retrieve and /v1/retrieve/trace/{query_id}."
              actions="Run retrieval probes, inspect score breakdowns, and debug missing context."
              how="The backend scores candidates, packs memories into a token budget, persists a trace, and the UI renders included and excluded candidates."
            />
            <PageWalkthrough
              title="Lifecycle"
              href="/lifecycle"
              purpose="Operate on memory quality over time."
              reads="Memory lifecycle, deleted memories, and candidate memory metadata."
              actions="Restore, promote, merge, publish, soft-delete, and run lifecycle-oriented workflows."
              how="Lifecycle actions update PostgreSQL first and coordinate with vector index changes so retrieval does not keep using stale entries."
            />
            <PageWalkthrough
              title="Ingest"
              href="/ingest"
              purpose="Manually create memories and test ingestion paths before wiring external systems."
              reads="Workspace/auth state and API responses from creation calls."
              actions="Create a memory directly or send a test event/observation."
              how="Direct memory creation writes a memory unit, then queues embedding/indexing work for the processor."
            />
            <PageWalkthrough
              title="Integrations"
              href="/integrations"
              purpose="Configure webhook sources and repair failed ingestion jobs."
              reads="Workspace integrations, recent observations, and DLQ entries."
              actions="Create/delete integration secrets, retry failed jobs, discard bad payloads, and review observations."
              how="Webhook integrations validate signatures, persist raw events, enqueue processor work, and move failures into the dead-letter queue."
            />
            <PageWalkthrough
              title="Tools"
              href="/tools"
              purpose="Register live HTTP tools that agents can invoke when memory alone is not enough."
              reads="Tool definitions, versions, secrets, and invocation history."
              actions="Create/update/test/delete tools, rollback versions, import/export tools, and inspect invocations."
              how="The backend stores schemas and encrypted auth material, validates inputs, calls the external endpoint, and records invocations."
            />
            <PageWalkthrough
              title="Agent Library"
              href="/agent-skills"
              purpose="Manage versioned skills, agent profiles, prompts, and reusable instructions."
              reads="Agent resources by type, target, name, and version."
              actions="Create, update, delete, copy, download, and restore prior resource versions."
              how="Resources are markdown-backed operating contracts that can be copied into agent runtimes or repo-local folders."
            />
            <PageWalkthrough
              title="Contradictions"
              href="/contradictions"
              purpose="Review conflicts between memories before agents rely on bad context."
              reads="Open contradiction flags and related memory content."
              actions="Accept both, dismiss, keep A, keep B, or bulk-dismiss low-risk flags."
              how="Resolution records the decision and can soft-delete the losing memory when an operator chooses one source of truth."
            />
            <PageWalkthrough
              title="Audit"
              href="/audit"
              purpose="Show who or what changed workspace state."
              reads="Workspace audit events."
              actions="Review operational history for compliance, debugging, and incident review."
              how="Backend handlers append audit records around sensitive operations such as keys, config, lifecycle, and integrations."
            />
            <PageWalkthrough
              title="Settings"
              href="/settings"
              purpose="Configure workspace connection, providers, keys, and administrative maintenance actions."
              reads="Workspace config, API key metadata, and local UI connection state."
              actions="Set API URL/workspace/API key, update model/provider config, create/revoke keys, re-index, and run promotion."
              how="Local connection values power the frontend; workspace config changes are patched to the backend and affect future retrieval/processing."
            />
          </Section>

          <Section id="dashboard" title="Dashboard">
            <p>
              The Dashboard answers: <strong>Is this workspace alive, growing, and trustworthy?</strong> It is the first
              screen to check after connecting a workspace or after a large ingestion run.
            </p>
            <DefinitionGrid
              items={[
                ["Backend ready", "A readiness check against the API. If it fails, most other screens will also fail."],
                ["Total/Episodic/Semantic", "The current memory mix. A healthy engineering workspace usually starts episodic-heavy and becomes more semantic as facts are promoted."],
                ["Pinned", "Memories protected from normal decay and pruning."],
                ["Created 7d/30d", "Recent ingestion velocity. A sudden drop usually means a webhook, processor, or credential problem."],
                ["Contradictions", "Open conflicts that should be reviewed before agents rely on the affected knowledge."],
                ["Avg importance/decay", "Quality and aging signals used by retrieval and lifecycle logic."],
              ]}
            />
            <p>
              Use the trend chart to confirm that ingestion and lifecycle events are occurring when expected. Use quick
              jumps when a metric points to a specific investigation path: Memory for content quality, Traces for retrieval
              quality, Lifecycle for governance, and Ingest for new data.
            </p>
          </Section>

          <Section id="memory-explorer" title="Memory Explorer">
            <p>
              The Memory Explorer is the main inspection surface for stored memory units. It is for answering: <strong>What
              does this workspace currently know?</strong>
            </p>
            <DefinitionGrid
              items={[
                ["List mode", "Fetches paginated memory units with filters such as type, pinned, importance, source, agent, user, repo, and time."],
                ["Search mode", "Sends the query to /v1/memory/search and returns ranked memory candidates."],
                ["Type filter", "Separates transient episodic evidence from durable semantic facts."],
                ["Scope filters", "Use agent_id or user_id to open that private layer, then add repo to include repo-specific variants without pulling unrelated repositories."],
                ["Importance filter", "Helps find high-value memories or low-value noise that may need cleanup."],
                ["Deleted/recoverable state", "Soft-deleted memories can remain visible to operators and may be restorable depending on policy."],
              ]}
            />
            <CodeBlock code={`# List recent semantic memories for a workspace
curl "{{API_URL}}/v1/memory?workspace_id={{WORKSPACE_ID}}&memory_type=semantic&limit=25&sort=created_at&direction=desc" \
  -H "x-api-key: {{API_KEY}}"

# Search memory units directly
curl -X POST {{API_URL}}/v1/memory/search \
  -H "x-api-key: {{API_KEY}}" \
  -H "Content-Type: application/json" \
  -d '{
    "workspace_id": "{{WORKSPACE_ID}}",
    "query": "release process and rollback rules",
    "mode": "hybrid",
    "limit": 10
  }'`} />
          </Section>

          <Section id="memory-detail" title="Memory Detail">
            <p>
              The Memory Detail page is the single-memory audit and correction view. Open it when you need to understand
              where a memory came from, why it exists, whether it should be trusted, or how it has changed over time.
            </p>
            <DefinitionGrid
              items={[
                ["Content", "The text agents will eventually see if the memory is retrieved."],
                ["Metadata", "Memory type, importance, decay, tags, scope, source references, created time, and update time."],
                ["Provenance", "A graph or relationship view that ties the memory back to source events and related memories."],
                ["History", "Version trail for updates and lifecycle actions so operators can reconstruct what changed."],
                ["Feedback", "Positive or negative signals that can influence future retrieval quality."],
                ["Actions", "Patch content/metadata, soft-delete, restore, promote to semantic, publish, or merge depending on state."],
              ]}
            />
            <CodeBlock code={`# Read one memory and its provenance
curl "{{API_URL}}/v1/memory/<memory_id>?workspace_id={{WORKSPACE_ID}}" \
  -H "x-api-key: {{API_KEY}}"

curl "{{API_URL}}/v1/memory/<memory_id>/provenance?workspace_id={{WORKSPACE_ID}}" \
  -H "x-api-key: {{API_KEY}}"

# Promote an important episodic memory to durable semantic knowledge
curl -X POST "{{API_URL}}/v1/memory/<memory_id>/promote?workspace_id={{WORKSPACE_ID}}" \
  -H "x-api-key: {{API_KEY}}"`} />
          </Section>

          <Section id="retrieval-trace" title="Retrieval Trace" tooltip="Explains search quality by showing candidate scoring and token packing decisions.">
            <p>
              Retrieval is not just search. It is a context-packing operation. MemoryOps finds candidate memories, scores
              them, filters by scope and workspace rules, packs the best items into a token budget, and stores a trace so
              the operator can debug the result.
            </p>
            <DefinitionGrid
              items={[
                ["Mode", "Search strategy, usually hybrid, that can combine semantic similarity and keyword signals."],
                ["Scope behavior", "When agent_id or user_id is supplied, retrieval returns matching scoped memories plus master workspace memory by default."],
                ["Candidate count", "How many memories were considered before final packing."],
                ["Included/excluded", "Whether a memory made it into the final context pack and why excluded memories were left out."],
                ["Score breakdown", "Semantic similarity, keyword rank, importance, recency, and source authority components."],
                ["Token budget", "Maximum estimated context size for the returned memory pack."],
                ["Trace TTL", "Traces are persisted for later inspection rather than disappearing after the request."],
              ]}
            />
            <CodeBlock code={`# Run retrieval and include an inline trace
curl -X POST {{API_URL}}/v1/retrieve \
  -H "x-api-key: {{API_KEY}}" \
  -H "Content-Type: application/json" \
  -d '{
    "workspace_id": "{{WORKSPACE_ID}}",
    "query": "what should the deploy bot remember before production release?",
    "mode": "hybrid",
    "token_budget": 1200,
    "include_trace": true,
    "agent_id": "deploy-bot",
    "repo": "org/backend"
  }'

# Fetch a persisted trace later
curl "{{API_URL}}/v1/retrieve/trace/<query_id>?workspace_id={{WORKSPACE_ID}}" \
  -H "x-api-key: {{API_KEY}}"`} />
            <p>
              Pass <code className="inline-code">agent_id</code> for agent-private memory, <code className="inline-code">user_id</code> for user-private memory, and <code className="inline-code">repo</code> when the memory should be limited to one repository. If you provide both <code className="inline-code">agent_id</code> and <code className="inline-code">user_id</code>, MemoryOps combines agent-only, user-only, shared agent+user, and matching repo-scoped variants. Set <code className="inline-code">include_master_memory</code> to <code className="inline-code">false</code> only when you need to suppress the default master workspace layer.
            </p>
          </Section>

          <Section id="ingest" title="Ingest Memories">
            <p>
              Ingestion is how information enters the memory plane. There are three common paths: direct memory creation,
              authenticated first-party observations, and third-party webhooks.
            </p>
            <DefinitionGrid
              items={[
                ["Direct memory", "Best for manual seeds, curated facts, tests, and migrations."],
                ["Agent observation", "Best for CI jobs, local agents, deploy bots, IDE agents, or any first-party process that can send an API key."],
                ["Webhook", "Best for external platforms such as GitHub, Slack, Jira, and Linear where MemoryOps validates a provider signature."],
                ["Processor queue", "After a write, background workers enrich, embed, score, detect contradictions, and index where needed."],
              ]}
            />
            <CodeBlock code={`# Create a memory directly
curl -X POST "{{API_URL}}/v1/memory?workspace_id={{WORKSPACE_ID}}" \
  -H "x-api-key: {{API_KEY}}" \
  -H "Content-Type: application/json" \
  -d '{
    "content": "We deploy every Tuesday at 10 AM UTC via the release pipeline.",
    "memory_type": "semantic",
    "importance_score": 0.85,
    "tags": ["deploy", "process"],
    "agent_id": "release-bot",
    "repo": "org/platform"
  }'

# Webhook endpoints by source
POST {{API_URL}}/v1/ingest/github/{{WORKSPACE_ID}}
POST {{API_URL}}/v1/ingest/slack/{{WORKSPACE_ID}}
POST {{API_URL}}/v1/ingest/jira/{{WORKSPACE_ID}}
POST {{API_URL}}/v1/ingest/linear/{{WORKSPACE_ID}}`} />
            <div className="mt-6 border-t border-line pt-6">
              <h3 className="font-semibold text-ink text-sm">The ingestion pipeline</h3>
              <FlowSteps
                steps={[
                  ["1", "Validate", "API key or webhook signature is checked before the event is trusted."],
                  ["2", "Persist", "Raw event or direct memory is stored in PostgreSQL with workspace and scope metadata."],
                  ["3", "Enqueue", "Redis-backed processing work is scheduled so HTTP requests can return quickly."],
                  ["4", "Process", "Workers parse content, extract entities, score importance, generate embeddings, and detect contradictions."],
                  ["5", "Index", "Qdrant receives vector points for semantic retrieval, while PostgreSQL remains the source of truth."],
                ]}
              />
            </div>
          </Section>

          <Section id="integrations" title="Integrations" tooltip="Webhook and observation operations for getting external activity into MemoryOps.">
            <p>
              The Integrations page has three operational tabs: <strong>Integration Health</strong>, <strong>Observations</strong>,
              and <strong>Dead Letter Queue</strong>. It is the place to prove that upstream systems are sending data and to
              repair events that failed processing.
            </p>
            <div className="grid gap-2 sm:grid-cols-2 lg:grid-cols-4">
              {(["github", "slack", "jira", "linear"] as const).map((source) => {
                const status = getIntegrationStatus(source);
                return (
                  <div key={source} className="flex items-center justify-between gap-3 rounded-lg border border-line bg-white px-3 py-2 text-sm">
                    <span className="font-medium capitalize text-ink">{source}</span>
                    <IntegrationStatusIndicator status={status.status} configured={status.configured} integration={status.integration} />
                  </div>
                );
              })}
            </div>
            <DefinitionGrid
              items={[
                ["Integration Health", "Shows configured sources, event counts, error counts, and last-event timestamps."],
                ["Observations", "Shows recent source=observation memories so first-party agent submissions are easy to verify."],
                ["Dead Letter Queue", "Failed jobs that can be retried after fixing transient dependencies or discarded when the payload is bad."],
                ["Webhook secret", "Shared secret used to validate provider signatures; it should match the value configured in the source platform."],
              ]}
            />
            <CodeBlock code={`# Register a GitHub webhook integration
curl -X POST {{API_URL}}/v1/workspaces/{{WORKSPACE_ID}}/integrations \
  -H "x-api-key: {{API_KEY}}" \
  -H "Content-Type: application/json" \
  -d '{"source": "github", "webhook_secret": "<shared-secret>"}'

# Retry a failed DLQ job after fixing the cause
curl -X POST {{API_URL}}/v1/workspaces/{{WORKSPACE_ID}}/dlq/<job_id>/retry \
  -H "x-api-key: {{API_KEY}}"`} />
          </Section>

          <Section id="tools" title="Tools" tooltip="HTTP endpoints agents can call to augment memory retrieval with live external data.">
            <p>
              Tools are governed HTTP endpoints. Use them when an agent needs live data that should not be stored as a
              long-lived memory, such as an incident status, a ticket lookup, inventory, weather, or a deployment check.
            </p>
            <DefinitionGrid
              items={[
                ["Tool definition", "Name, description, endpoint URL, HTTP method, input schema, and enabled state."],
                ["Input schema", "JSON Schema contract agents must satisfy before MemoryOps invokes the tool."],
                ["Auth secret", "Optional encrypted secret sent to the tool endpoint according to the configured auth behavior."],
                ["Version history", "Changes to a tool can be inspected and rolled back."],
                ["Invocation log", "Records live calls so operators can debug failures and unexpected agent behavior."],
                ["Test action", "Sends a live request from the UI to verify endpoint behavior before agents depend on it."],
              ]}
            />
            <CodeBlock code={`curl -X POST {{API_URL}}/v1/workspaces/{{WORKSPACE_ID}}/tools \
  -H "x-api-key: {{API_KEY}}" \
  -H "Content-Type: application/json" \
  -d '{
    "name": "deployment_status",
    "description": "Returns current deployment status for a service",
    "endpoint_url": "https://api.example.com/deployments/status",
    "http_method": "POST",
    "input_schema": {
      "type": "object",
      "properties": {
        "service": { "type": "string" }
      },
      "required": ["service"]
    },
    "enabled": true
  }'

curl -X POST {{API_URL}}/v1/workspaces/{{WORKSPACE_ID}}/tools/deployment_status/test \
  -H "x-api-key: {{API_KEY}}" \
  -H "Content-Type: application/json" \
  -d '{"service": "api"}'`} />
          </Section>

          <Section id="agent-skills" title="Agent Library" tooltip="Versioned skills, prompts, agent profiles, and instructions for agent clients.">
            <p>
              Agent Library resources are not memories themselves. They are markdown instructions and prompt assets that
              tell an assistant when to store memory, when to retrieve memory, which scopes to include, and how to avoid
              polluting the workspace with noisy or short-lived facts.
            </p>
            <DefinitionGrid
              items={[
                ["Type", "Skill, agent, prompt, or instruction."],
                ["Target", "The agent family or client, such as Claude, Gemini, OpenAI, or a generic library."],
                ["Version", "Each save creates an immutable snapshot that can be restored later."],
                ["Content", "The markdown behavior contract or prompt body for the agent."],
                ["Best use", "Store stable user/project preferences, architectural decisions, repeated incident learnings, and durable workflow rules."],
              ]}
            />
            <Callout>
              Good resources should be opinionated. They should tell the agent to retrieve before answering project-specific
              questions and to store only durable knowledge, not every intermediate thought or temporary task.
            </Callout>
          </Section>

          <Section id="contradictions" title="Contradictions" tooltip="Review queue for memories that may disagree and need operator resolution.">
            <p>
              Contradictions protect agents from blindly using conflicting facts. A conflict may be legitimate, stale, or
              a sign that one memory should replace another. Operators decide the outcome.
            </p>
            <DefinitionGrid
              items={[
                ["Accept both", "Use when both facts are true in different scopes, times, repositories, or contexts."],
                ["Dismiss", "Use when the flag is not actionable but neither memory should be changed."],
                ["Keep A / Keep B", "Use when one memory is the correct source of truth and the other should be soft-deleted."],
                ["Bulk dismiss", "Use for low-risk false positives after sampling enough examples."],
              ]}
            />
            <CodeBlock code={`# Resolve via API — keep memory A, discard memory B
curl -X POST {{API_URL}}/v1/workspaces/{{WORKSPACE_ID}}/contradictions/<flag_id>/resolve \
  -H "x-api-key: {{API_KEY}}" \
  -H "Content-Type: application/json" \
  -d '{"resolution": "keep_a"}'`} />
          </Section>

          <Section id="lifecycle" title="Lifecycle & Decay" tooltip="Rules that age, prune, and promote memories over time.">
            <p>
              Lifecycle is how MemoryOps prevents the workspace from becoming an unbounded junk drawer. The platform uses
              importance, recency, memory type, feedback, pinning, and explicit operator actions to decide what should stay
              prominent, become semantic, be restored, or be removed from retrieval.
            </p>
            <DefinitionGrid
              items={[
                ["Decay", "Time-based weakening of memories, primarily affecting episodic material."],
                ["Promotion", "Turning valuable episodic evidence into durable semantic knowledge."],
                ["Pruning", "Removing low-value or stale memories from active retrieval according to workspace policy."],
                ["Soft delete", "Marks a memory as deleted while preserving recovery and audit options."],
                ["Restore", "Returns a soft-deleted memory to active use."],
                ["Merge", "Combines overlapping memories into a cleaner source of truth."],
                ["Publish", "Marks a memory as validated or ready for broader use depending on workspace policy."],
              ]}
            />
            <CodeBlock code={`# Update decay settings
curl -X PATCH {{API_URL}}/v1/workspaces/{{WORKSPACE_ID}}/config \
  -H "x-api-key: {{API_KEY}}" \
  -H "Content-Type: application/json" \
  -d '{
    "decay_half_life_days": 30,
    "pruning_threshold": 0.1
  }'

# Run a workspace promotion cycle
curl -X POST {{API_URL}}/v1/workspaces/{{WORKSPACE_ID}}/promote \
  -H "x-api-key: {{API_KEY}}"`} />
          </Section>

          <Section id="audit" title="Audit">
            <p>
              Audit is the accountability layer. Use it when you need to answer who changed a key, which automation updated
              workspace config, when a memory was deleted, or what happened during an incident investigation.
            </p>
            <DefinitionGrid
              items={[
                ["Actor", "The API key, operator, or system path associated with the event."],
                ["Action", "What happened, such as create key, revoke key, update config, resolve contradiction, or lifecycle operation."],
                ["Target", "The workspace, memory, integration, tool, or key affected by the action."],
                ["Timestamp", "When the backend recorded the action."],
              ]}
            />
            <CodeBlock code={`curl "{{API_URL}}/v1/workspaces/{{WORKSPACE_ID}}/audit?limit=50" \
  -H "x-api-key: {{API_KEY}}"`} />
          </Section>

          <Section id="settings" title="Settings">
            <p>
              Settings contains both local connection state for the frontend and persisted workspace configuration for the
              backend. Local values decide how the browser connects. Workspace config decides how the system behaves.
            </p>
            <DefinitionGrid
              items={[
                ["API URL", "Frontend base URL used by the Control Center. Code examples in this guide use the same value."],
                ["Workspace ID", "The workspace boundary for all guide examples and UI calls."],
                ["API key", "Stored client-side for authenticated requests from the browser."],
                ["LLM provider/model", "Used by backend intelligence features such as extraction, contradiction analysis, or summarization where configured."],
                ["Embedding provider/model", "Controls how memory text becomes vectors for semantic retrieval."],
                ["Contradiction mode", "Controls whether conflicts are surfaced, quarantined, or handled according to policy."],
                ["Keys", "Create and revoke credentials for agents, operators, and integrations that call protected endpoints."],
                ["Maintenance", "Re-index vectors or trigger promotion when backend state needs refresh."],
              ]}
            />
            <CodeBlock code={`curl -X PATCH {{API_URL}}/v1/workspaces/{{WORKSPACE_ID}}/config \
  -H "x-api-key: {{API_KEY}}" \
  -H "Content-Type: application/json" \
  -d '{
    "llm_provider": "openai",
    "llm_model": "gpt-4o-mini",
    "embedding_provider": "openai",
    "embedding_model": "text-embedding-3-small",
    "contradiction_mode": "quarantine"
  }'

curl -X POST {{API_URL}}/v1/workspaces/{{WORKSPACE_ID}}/reindex \
  -H "x-api-key: {{API_KEY}}"`} />
          </Section>

          <Section id="authentication" title="Authentication">
            <p>
              Protected endpoints require an API key in the <code className="inline-code">x-api-key</code> header. The key
              is bound to a workspace, so a valid key for one workspace should not operate on another workspace.
            </p>
            <CodeBlock code={`curl {{API_URL}}/v1/workspaces/me \
  -H "x-api-key: {{API_KEY}}"

curl {{API_URL}}/v1/workspaces/{{WORKSPACE_ID}} \
  -H "x-api-key: {{API_KEY}}"`} />
            <DefinitionGrid
              items={[
                ["401 Unauthorized", "The key is missing, malformed, expired, revoked, or otherwise invalid."],
                ["403 Forbidden", "The key is valid but not authorized for the requested workspace or action."],
                ["Workspace creation", "Bootstrap workspace creation is separate from normal API-key auth and should be protected by the workspace creation secret."],
                ["Browser storage", "The frontend uses the configured key for UI calls. Treat any browser with the key as an operator session."],
              ]}
            />
          </Section>

          <Section id="vscode" title="VS Code Extension — Coming Soon" status={null}>
            <p>
              A first-party <strong>MemoryOps VS Code extension</strong> is planned. The goal is to bring workspace memory
              directly into the editor so coding agents and developers can retrieve relevant project context without
              leaving VS Code.
            </p>
            <Callout>
              Until the extension is published, connect editor-based agents through the MCP server or call the REST API
              directly. The settings below show the intended shape for editor configuration.
            </Callout>
            <CodeBlock code={`// planned settings.json shape
{
  "memoryops.apiUrl": "{{API_URL}}",
  "memoryops.workspaceId": "{{WORKSPACE_ID}}",
  "memoryops.apiKey": "{{API_KEY}}",
  "memoryops.defaultTopK": 5,
  "memoryops.defaultScope": {
    "agent_id": "copilot",
    "repo": "owner/repo"
  }
}`} />
            <p>
              The best editor workflow is retrieval-before-action: pull repository-specific memory before editing code,
              then store only durable discoveries such as recurring bugs, architectural constraints, or confirmed decisions.
            </p>
          </Section>

          <Section id="claude-desktop" title="Claude Desktop (MCP)" tooltip="MCP lets MemoryOps expose memory read and write tools directly to compatible agent clients." status={null}>
            <p>
              MemoryOps exposes an MCP server so Claude Desktop can read and write memories during conversations. Add the
              server to your Claude Desktop configuration file and restart Claude Desktop after editing the file.
            </p>
            <p className="text-sm text-ink/70">
              Configuration file location: <code className="inline-code">~/.config/claude/claude_desktop_config.json</code>{" "}
              on macOS/Linux or <code className="inline-code">%APPDATA%\Claude\claude_desktop_config.json</code> on Windows.
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
            <DefinitionGrid
              items={[
                ["memory_retrieve", "Agent asks MemoryOps for relevant context before answering or changing code."],
                ["memory_store", "Agent stores durable observations with optional tags, importance, and scope."],
                ["Scope", "Pass agent_id for agent-private memory, user_id for user-private memory, and repo only when the memory should apply to one repository. Retrieval combines those scoped layers with master workspace memory unless include_master_memory is false."],
                ["Safety", "Do not store secrets, transient scratchpad content, or one-off task state as durable memory."],
              ]}
            />
          </Section>

          <Section id="agent-observations" title="Agent Observations" tooltip="First-party agent-submitted memories sent directly into the MemoryOps ingest pipeline.">
            <p>
              Agent Observations let any process — a CI pipeline, local coding agent, deploy bot, CLI script, or scheduled
              automation — push structured observations directly into MemoryOps without relying on a third-party webhook
              format.
            </p>
            <CodeBlock code={`curl -X POST {{API_URL}}/v1/ingest/observation \
  -H "x-api-key: {{API_KEY}}" \
  -H "Content-Type: application/json" \
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
            <p>
              The endpoint returns <code className="inline-code">202 Accepted</code> with a raw event id. Processing is
              asynchronous, so the observation may not appear in search until workers have embedded and indexed it.
            </p>
            <CodeBlock code={`{ "id": "<uuid>", "status": "queued" }`} />
            <Callout>
              Use observations for first-party agents you control. Use webhooks for external systems that already sign and
              send events to fixed provider URLs.
            </Callout>
          </Section>

          <Section id="openwebui" title="OpenWebUI" status={null}>
            <p>
              Add MemoryOps as an OpenWebUI function to enrich chat messages with retrieved workspace memories. The
              function should retrieve context before the model responds and should optionally store durable user or project
              preferences after the model confirms them.
            </p>
            <ol className="grid gap-2 pl-4 text-ink/80" style={{ listStyleType: "decimal" }}>
              <li>In OpenWebUI, navigate to <strong>Workspace → Functions</strong>.</li>
              <li>Create a function and paste the MemoryOps function code.</li>
              <li>Set <code className="inline-code">MEMORYOPS_API_URL</code>, <code className="inline-code">MEMORYOPS_WORKSPACE_ID</code>, and <code className="inline-code">MEMORYOPS_API_KEY</code>.</li>
              <li>Choose a conservative <code className="inline-code">TOP_K</code> value first, then increase it only if answers lack context.</li>
              <li>Enable the function for the models that should use governed workspace memory.</li>
            </ol>
            <CodeBlock code={`# Function valves
MEMORYOPS_API_URL = "{{API_URL}}"
MEMORYOPS_WORKSPACE_ID = "{{WORKSPACE_ID}}"
MEMORYOPS_API_KEY = "{{API_KEY}}"
TOP_K = 5
TOKEN_BUDGET = 1200`} />
          </Section>

          <Section id="direct-api" title="Direct REST API">
            <p>
              Any HTTP client can talk to MemoryOps directly. The protected API uses <code className="inline-code">x-api-key</code>{" "}
              auth and usually takes <code className="inline-code">workspace_id</code> either in the path, query string, or
              request body depending on the endpoint family.
            </p>
            <EndpointTable
              rows={[
                { method: "GET", path: "/health/ready", purpose: "Fast readiness check for the API." },
                { method: "GET", path: "/health/system", purpose: "Dependency health for Postgres, Redis, Qdrant, and provider checks." },
                { method: "GET", path: "/v1/workspaces/{id}", purpose: "Read workspace metadata." },
                { method: "PATCH", path: "/v1/workspaces/{id}/config", purpose: "Update workspace behavior." },
                { method: "GET", path: "/v1/memory", purpose: "List memory units with filters." },
                { method: "POST", path: "/v1/memory", purpose: "Create a memory unit." },
                { method: "POST", path: "/v1/memory/search", purpose: "Search ranked memory candidates." },
                { method: "POST", path: "/v1/retrieve", purpose: "Build an agent-ready memory context pack." },
                { method: "GET", path: "/v1/retrieve/trace/{query_id}", purpose: "Read a persisted retrieval trace." },
                { method: "GET", path: "/v1/workspaces/{id}/export", purpose: "Export workspace memories." },
              ]}
            />
            <CodeBlock code={`# Minimal retrieve request for an agent
curl -X POST {{API_URL}}/v1/retrieve \
  -H "x-api-key: {{API_KEY}}" \
  -H "Content-Type: application/json" \
  -d '{
    "workspace_id": "{{WORKSPACE_ID}}",
    "query": "what should I know before changing the auth middleware?",
    "mode": "hybrid",
    "token_budget": 1500,
    "include_trace": true
  }'`} />
          </Section>

          <Section id="backend-architecture" title="Backend Architecture">
            <p>
              MemoryOps is a Rust service built around explicit operational boundaries. The frontend is a Control Center;
              the Rust API and worker pipeline own state changes, retrieval, and background processing.
            </p>
            <FlowSteps
              steps={[
                ["UI", "React Control Center", "Reads and writes through typed API clients, renders metrics, workflows, and operator guardrails."],
                ["API", "Axum HTTP service", "Authenticates requests, applies rate limits, validates payloads, routes protected workspace operations, and exposes health checks."],
                ["DB", "PostgreSQL", "System of record for workspaces, keys, memory units, source events, config, audit, traces, tools, and integration metadata."],
                ["Queue", "Redis", "Decouples ingestion from expensive enrichment and embedding work."],
                ["Vector", "Qdrant", "Stores embeddings for semantic retrieval and is rebuilt through re-indexing when needed."],
                ["Workers", "Processor/scheduler", "Handle enrichment, embeddings, contradiction detection, lifecycle passes, and delayed work."],
              ]}
            />
            <p>
              This separation is important: if retrieval is weak, inspect traces and Qdrant/embedding health; if events are
              missing, inspect integrations and Redis/processor health; if UI data looks wrong, verify the underlying API
              endpoint before assuming the frontend state is incorrect.
            </p>
          </Section>

          <Section id="export-import" title="Export & Import" tooltip="Workspace backup and restore flows for memory migration or recovery.">
            <p>
              Export and import are workspace mobility tools. Use them for backups, migrations, local testing, disaster
              recovery drills, or seeding a new environment with known memory data.
            </p>
            <CodeBlock code={`# Export workspace memories to NDJSON
curl {{API_URL}}/v1/workspaces/{{WORKSPACE_ID}}/export \
  -H "x-api-key: {{API_KEY}}" \
  -o memories.ndjson

# Import an NDJSON export
curl -X POST {{API_URL}}/v1/workspaces/{{WORKSPACE_ID}}/import \
  -H "x-api-key: {{API_KEY}}" \
  -H "Content-Type: application/x-ndjson" \
  --data-binary @memories.ndjson`} />
            <Callout>
              After a large import, run re-index if search results look incomplete. Imports restore memory records, while
              vector search quality depends on embeddings and Qdrant index state.
            </Callout>
          </Section>

          <Section id="troubleshooting" title="Troubleshooting">
            <p>
              Start with health, then narrow by workflow. Health tells you whether dependencies are reachable. The page
              you were using tells you which subsystem to inspect next.
            </p>
            <CodeBlock code={`curl {{API_URL}}/health/system \
  -H "x-api-key: {{API_KEY}}"

# Expected shape
{
  "status": "healthy",
  "checks": [
    { "name": "postgres", "status": "ok", "latency_ms": 2 },
    { "name": "redis",    "status": "ok", "latency_ms": 1 },
    { "name": "qdrant",   "status": "ok", "latency_ms": 5 }
  ]
}`} />
            <p className="font-medium text-ink">Common issues</p>
            <ul className="grid gap-2 pl-4 text-sm text-ink/80" style={{ listStyleType: "disc" }}>
              <li><strong>401 Unauthorized</strong> — Check that <code className="inline-code">x-api-key</code> is present, copied correctly, and not revoked.</li>
              <li><strong>403 Forbidden</strong> — Confirm the key belongs to the workspace you are querying.</li>
              <li><strong>Webhook signature mismatch</strong> — Ensure the provider secret exactly matches the MemoryOps integration secret and has no whitespace/encoding drift.</li>
              <li><strong>Observations queued but not searchable</strong> — Check Redis and processor worker health; embedding/indexing is asynchronous.</li>
              <li><strong>No search results</strong> — Confirm memories exist, embeddings were generated, filters are not too narrow, and Qdrant is healthy. Then run re-index.</li>
              <li><strong>Bad retrieval quality</strong> — Use Traces to inspect candidates, score breakdowns, token budget, scope filters, and whether workspace-pool retrieval is enabled.</li>
              <li><strong>DLQ filling up</strong> — Expand failed jobs, fix the parser/signature/provider issue, retry a sample, then retry the rest.</li>
              <li><strong>MCP not connecting</strong> — Verify <code className="inline-code">memoryops-mcp</code> is on PATH, env vars are set, and the client was restarted after config changes.</li>
              <li><strong>429 rate limiting</strong> — Reduce client retry loops or tune workspace/server limits before increasing agent concurrency.</li>
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

function MiniCard({ title, accent, children }: { title: string; accent: "blue" | "green"; children: React.ReactNode }) {
  return (
    <div className="rounded-lg border border-line bg-soft/50 p-4">
      <h3 className="font-semibold text-ink flex items-center gap-2">
        <span className={cn("h-2 w-2 rounded-full", accent === "blue" ? "bg-blue-500" : "bg-emerald-500")} />
        {title}
      </h3>
      <p className="mt-2 text-xs text-ink/75 leading-relaxed">{children}</p>
    </div>
  );
}

function DefinitionGrid({ items }: { items: Array<[string, string]> }) {
  return (
    <div className="grid gap-3 md:grid-cols-2">
      {items.map(([term, description]) => (
        <div key={term} className="rounded-lg border border-line bg-white p-3.5">
          <p className="text-xs font-semibold uppercase tracking-wide text-ink/50">{term}</p>
          <p className="mt-1 text-xs leading-relaxed text-ink/75">{description}</p>
        </div>
      ))}
    </div>
  );
}

function PageWalkthrough({
  title,
  href,
  purpose,
  reads,
  actions,
  how,
}: {
  title: string;
  href: string;
  purpose: string;
  reads: string;
  actions: string;
  how: string;
}) {
  return (
    <div className="rounded-lg border border-line bg-white p-4">
      <div className="flex flex-wrap items-center justify-between gap-2">
        <h3 className="text-sm font-semibold text-ink">{title}</h3>
        <a href={href} className="text-xs font-medium text-accent-strong underline underline-offset-2">Open page</a>
      </div>
      <p className="mt-2 text-sm text-ink/80">{purpose}</p>
      <dl className="mt-3 grid gap-2 text-xs text-ink/70 md:grid-cols-3">
        <div>
          <dt className="font-semibold text-ink/55">Reads</dt>
          <dd className="mt-1">{reads}</dd>
        </div>
        <div>
          <dt className="font-semibold text-ink/55">Actions</dt>
          <dd className="mt-1">{actions}</dd>
        </div>
        <div>
          <dt className="font-semibold text-ink/55">How it works</dt>
          <dd className="mt-1">{how}</dd>
        </div>
      </dl>
    </div>
  );
}

function FlowSteps({ steps }: { steps: Array<[string, string, string]> }) {
  return (
    <div className="grid gap-3 md:grid-cols-2 xl:grid-cols-3">
      {steps.map(([step, title, description]) => (
        <div key={`${step}-${title}`} className="rounded-lg border border-line bg-soft/30 p-3.5">
          <h4 className="text-xs font-semibold text-ink flex items-center gap-1.5">
            <span className="h-5 min-w-5 rounded-full bg-accent/15 px-1.5 text-accent flex items-center justify-center text-[10px] font-bold">{step}</span>
            {title}
          </h4>
          <p className="mt-1.5 text-xs text-ink/75 leading-relaxed">{description}</p>
        </div>
      ))}
    </div>
  );
}

function EndpointTable({ rows }: { rows: Array<{ method: string; path: string; purpose: string }> }) {
  return (
    <div className="overflow-hidden rounded-lg border border-line bg-white">
      <table className="min-w-full divide-y divide-line text-left text-xs">
        <thead className="bg-soft/60 text-ink/55">
          <tr>
            <th scope="col" className="px-3 py-2 font-semibold uppercase tracking-wide">Method</th>
            <th scope="col" className="px-3 py-2 font-semibold uppercase tracking-wide">Path</th>
            <th scope="col" className="px-3 py-2 font-semibold uppercase tracking-wide">Purpose</th>
          </tr>
        </thead>
        <tbody className="divide-y divide-line text-ink/75">
          {rows.map((row) => (
            <tr key={`${row.method}-${row.path}`}>
              <td className="whitespace-nowrap px-3 py-2 font-mono font-semibold text-ink">{row.method}</td>
              <td className="px-3 py-2 font-mono text-ink/80">{row.path}</td>
              <td className="px-3 py-2">{row.purpose}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
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
