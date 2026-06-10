import { ArrowLeft, Check, Clock3, Database, GitCommit, GitMerge, Plus, Save, Share2, X } from "lucide-react";
import { useEffect, useMemo, useState, type FormEvent } from "react";
import { Link, useParams, useSearchParams } from "react-router-dom";

import type { MemoryScope, MemoryUnit, MemoryVersion, ProvenanceGraph, ProvenanceNode } from "../api/types";
import { EmptyState } from "../components/EmptyState";
import { EntityChip } from "../components/EntityChip";
import { FeedbackPanel } from "../components/FeedbackPanel";
import { InlineError } from "../components/InlineError";
import { MemoryLifecycleActions } from "../components/MemoryLifecycleActions";
import { Badge } from "../components/ui/badge";
import { Button } from "../components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "../components/ui/card";
import { Input } from "../components/ui/input";
import { Skeleton } from "../components/ui/skeleton";
import { HelpTooltip, InfoLabel, Tooltip, TooltipContent, TooltipTrigger } from "../components/ui/tooltip";
import { useMemoryDetail, useMemoryHistory, useMemoryProvenance, usePublishMemory, useUpdateMemory } from "../hooks/use-memory";
import { formatCount, formatDateTime, formatRelativeTime, formatScore } from "../lib/format";
import { validateImportanceScore } from "../lib/validation";
import { useAppStore } from "../store/app-store";

export function MemoryDetail() {
  const { id } = useParams<{ id?: string }>();
  const [searchParams] = useSearchParams();
  const workspaceId = useAppStore((state) => state.workspaceId);
  const memoryQuery = useMemoryDetail(workspaceId, id);
  const provenanceQuery = useMemoryProvenance(workspaceId, id);
  const updateMemory = useUpdateMemory(workspaceId);
  const publishMemory = usePublishMemory(workspaceId);
  const historyQuery = useMemoryHistory(workspaceId, id);
  const memory = memoryQuery.data;
  const scope = useMemo(() => normalizeScope(memory, workspaceId), [memory, workspaceId]);
  const [draftTags, setDraftTags] = useState<string[]>([]);
  const [tagInput, setTagInput] = useState("");
  const [importanceDraft, setImportanceDraft] = useState(0);
  const [importanceError, setImportanceError] = useState<string | null>(null);
  const initialQueryId = searchParams.get("query_id") ?? "";

  useEffect(() => {
    if (memory) {
      setDraftTags(memory.tags);
      setImportanceDraft(memory.importance_score);
      setImportanceError(null);
    }
  }, [memory]);

  if (!id) {
    return null;
  }

  function addTag(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const nextTag = tagInput.trim();
    if (nextTag.length === 0 || draftTags.includes(nextTag)) {
      setTagInput("");
      return;
    }

    setDraftTags((tags) => [...tags, nextTag]);
    setTagInput("");
  }

  function saveTags() {
    if (!memory) {
      return;
    }

    updateMemory.mutate({ id: memory.id, patch: { tags: draftTags } });
  }

  function saveImportance() {
    if (!memory) {
      return;
    }

    const message = validateImportanceScore(importanceDraft);
    setImportanceError(message);
    if (message) {
      return;
    }

    updateMemory.mutate({ id: memory.id, patch: { importance_score: importanceDraft } });
  }

  function togglePinned() {
    if (!memory) {
      return;
    }

    updateMemory.mutate({ id: memory.id, patch: { pinned: !memory.pinned } });
  }

  function publishToWorkspacePool() {
    if (!memory) {
      return;
    }

    publishMemory.mutate({ id: memory.id });
  }

  return (
    <div className="mx-auto grid max-w-7xl gap-5">
      <div>
        <Button asChild variant="ghost" size="sm">
          <Link to="/memory" data-testid="detail-back-link">
            <ArrowLeft className="h-4 w-4" aria-hidden="true" />
            Explorer
          </Link>
        </Button>
      </div>

      {memoryQuery.isLoading ? <DetailSkeleton /> : null}
      {memoryQuery.isError ? <InlineError message={errorMessage(memoryQuery.error)} /> : null}

      {memory ? (
        <>
          <header className="grid gap-4 lg:grid-cols-[1fr_auto] lg:items-start">
            <div>
              <div className="flex flex-wrap items-center gap-2">
                <TooltipBadge
                  variant={memory.memory_type === "semantic" ? "teal" : "rust"}
                  tooltip={memory.memory_type === "semantic"
                    ? "Durable knowledge promoted from recurring or important episodic memories."
                    : "Short-lived event-derived memories from raw activity like commits, PRs, messages, tickets, and agent observations."}
                >
                  {memory.memory_type === "semantic" ? "Semantic" : "Episodic"}
                </TooltipBadge>
                <TooltipBadge
                  variant={memory.scope_visibility === "workspace" ? "green" : "gray"}
                  tooltip={memory.scope_visibility === "workspace"
                    ? "Workspace-visible memories are available across agents and users in this workspace, not just the original private scope."
                    : "Scoped memory. Retrieval should respect the agent, user, or repo scope attached to this memory."}
                >
                  {memory.scope_visibility === "workspace" ? "Workspace Pool" : "Private"}
                </TooltipBadge>
                {memory.importance_overridden ? <TooltipBadge variant="amber" tooltip="This memory has a manual priority override set by an operator.">Overridden</TooltipBadge> : null}
                {memory.memory_type === "semantic" && memory.corroboration_count > 1 ? (
                  <TooltipBadge variant="purple" tooltip="Corroborating source episodes or events that support this semantic memory.">
                    ⬡ {memory.corroboration_count} sources
                  </TooltipBadge>
                ) : null}
                <span className="text-sm text-ink/55">Updated {formatDateTime(memory.updated_at)}</span>
                {memory.promoted_at ? <span className="text-sm text-ink/55">Promoted {formatRelativeTime(memory.promoted_at)}</span> : null}
              </div>
              <h1 className="mt-3 text-2xl font-semibold tracking-normal text-ink">Memory Detail</h1>
            </div>
            <div className="grid gap-2 justify-items-end">
              <div className="flex flex-wrap justify-end gap-2">
              {memory.memory_type === "semantic" && memory.scope_visibility === "private" ? (
                <Tooltip>
                  <TooltipTrigger asChild>
                    <Button type="button" variant="secondary" data-testid="detail-publish-button" onClick={publishToWorkspacePool} disabled={publishMemory.isPending}>
                      <Share2 className="h-4 w-4" aria-hidden="true" />
                      Publish
                    </Button>
                  </TooltipTrigger>
                  <TooltipContent>Makes this semantic memory available to the wider workspace pool so other agents and scopes can retrieve it.</TooltipContent>
                </Tooltip>
              ) : null}
              <Tooltip>
                <TooltipTrigger asChild>
                  <Button type="button" variant={memory.pinned ? "secondary" : "default"} data-testid="detail-pin-toggle" onClick={togglePinned} disabled={updateMemory.isPending}>
                    <Check className="h-4 w-4" aria-hidden="true" />
                    {memory.pinned ? "Pinned" : "Pin"}
                  </Button>
                </TooltipTrigger>
                <TooltipContent>{memory.pinned ? "Pinned memories stay protected from normal decay and pruning." : "Protect this memory from normal decay and pruning."}</TooltipContent>
              </Tooltip>
              </div>
              <MemoryLifecycleActions workspaceId={workspaceId} memory={memory} />
            </div>
          </header>

          {updateMemory.isError ? <InlineError message={errorMessage(updateMemory.error)} /> : null}
          {publishMemory.isError ? <InlineError message={errorMessage(publishMemory.error)} /> : null}

          <section className="grid gap-4 xl:grid-cols-[1fr_22rem]">
            <Card>
              <CardHeader>
                <CardTitle className="flex items-center gap-1.5">
                  <span>Content</span>
                  <HelpTooltip label="Content">The normalized memory text that retrieval and lifecycle logic operate on.</HelpTooltip>
                </CardTitle>
              </CardHeader>
              <CardContent>
                <div className="whitespace-pre-wrap rounded-lg border border-line bg-soft p-4 text-sm leading-6 text-ink">{memory.content}</div>
              </CardContent>
            </Card>

            <div className="grid gap-4">
              <Card>
                <CardHeader>
                  <CardTitle className="flex items-center gap-1.5">
                    <span>Scores</span>
                    <HelpTooltip label="Scores">Priority, decay, access, and relevance signals used to rank and manage this memory.</HelpTooltip>
                  </CardTitle>
                </CardHeader>
                <CardContent className="grid gap-3">
                  <ScoreLine label="Importance" helpText="Average priority score used by retrieval, lifecycle, and promotion logic." value={formatScore(memory.importance_score)} />
                  <ScoreLine label="Decay" helpText="How strongly this memory is aging out of retrieval. Lower scores are more likely to be pruned or deprioritized." value={formatScore(memory.decay_score)} />
                  <ScoreLine label="Access count" helpText="How many times retrieval or operator workflows have accessed this memory." value={formatCount(memory.access_count)} />
                  {memory.promoted_at ? <ScoreLine label="Promoted" value={formatRelativeTime(memory.promoted_at)} /> : null}
                  <RelevanceMeter score={memory.relevance_score} />
                </CardContent>
              </Card>
              <FeedbackPanel workspaceId={workspaceId} memoryId={memory.id} initialQueryId={initialQueryId} />
            </div>
          </section>

          <section className="grid gap-4 xl:grid-cols-2">
            <Card>
              <CardHeader>
                <CardTitle className="flex items-center gap-1.5">
                  <span>Entities</span>
                  <HelpTooltip label="Entities">Extracted structured topics, repositories, people, or labels associated with this memory.</HelpTooltip>
                </CardTitle>
              </CardHeader>
              <CardContent>
                {memory.entities && memory.entities.length > 0 ? (
                  <div className="flex flex-wrap gap-2">
                    {memory.entities.map((entity) => (
                      <EntityChip key={`${entity.entity_type}:${entity.value}`} entity={entity} />
                    ))}
                  </div>
                ) : (
                  <EmptyState title="Entity extraction is quiet" message="Entity chips will appear here as processed memories include structured context." />
                )}
              </CardContent>
            </Card>

            <Card>
              <CardHeader>
                <CardTitle className="flex items-center gap-1.5">
                  <span>Scope</span>
                  <HelpTooltip label="Scope">Where this memory is allowed to apply. Retrieval should respect these workspace, agent, user, and repo boundaries.</HelpTooltip>
                </CardTitle>
              </CardHeader>
              <CardContent className="grid gap-3 sm:grid-cols-2">
                <ScopeField label="workspace_id" helpText="Workspace boundary for this memory record." value={scope.workspace_id} />
                <ScopeField label="agent_id" helpText="Agent scope attached to this memory, if any." value={scope.agent_id ?? "workspace-wide"} />
                <ScopeField label="user_id" helpText="User scope attached to this memory, if any." value={scope.user_id ?? "all users"} />
                <ScopeField label="repo" helpText="Repository scope attached to this memory, if any." value={scope.repo ?? "all repos"} />
              </CardContent>
            </Card>
          </section>

          <section data-testid="provenance-panel">
            <Card>
              <CardHeader>
                <CardTitle className="flex items-center gap-1.5">
                  <span>Lineage</span>
                  <HelpTooltip label="Lineage">Shows how this memory was created, promoted, merged, accessed, or derived from source events.</HelpTooltip>
                </CardTitle>
              </CardHeader>
              <CardContent>
                {provenanceQuery.isLoading ? <Skeleton className="h-48 w-full" /> : null}
                {provenanceQuery.isError ? <InlineError title="Lineage unavailable" message={errorMessage(provenanceQuery.error)} /> : null}
                {provenanceQuery.data ? <ProvenanceTree graph={provenanceQuery.data} /> : null}
              </CardContent>
            </Card>
          </section>

          <section className="grid gap-4 xl:grid-cols-2">
            <Card>
              <CardHeader>
                <CardTitle className="flex items-center gap-1.5">
                  <span>Tags</span>
                  <HelpTooltip label="Tags">Operator and system labels that help organize, filter, and retrieve this memory.</HelpTooltip>
                </CardTitle>
              </CardHeader>
              <CardContent className="grid gap-4">
                <div className="flex min-h-9 flex-wrap gap-2">
                  {draftTags.length > 0 ? (
                    draftTags.map((tag) => (
                      <Badge key={tag} variant="gray" className="gap-1 pr-1">
                        {tag}
                        <button
                          type="button"
                          data-testid={`remove-tag-${tag}`}
                          aria-label={`Remove ${tag}`}
                          className="rounded p-0.5 hover:bg-zinc-200 focus:outline-none focus:ring-2 focus:ring-accent"
                          onClick={() => setDraftTags((tags) => tags.filter((value) => value !== tag))}
                        >
                          <X className="h-3 w-3" aria-hidden="true" />
                        </button>
                      </Badge>
                    ))
                  ) : (
                    <span className="text-sm text-ink/55">Tags will collect operator labels here.</span>
                  )}
                </div>
                <form className="flex gap-2" onSubmit={addTag}>
                  <Input data-testid="tag-input" value={tagInput} onChange={(event) => setTagInput(event.target.value)} placeholder="Add tag" />
                  <Button type="submit" variant="secondary" data-testid="tag-add-button">
                    <Plus className="h-4 w-4" aria-hidden="true" />
                    Add
                  </Button>
                </form>
                <Button type="button" data-testid="tags-save-button" onClick={saveTags} disabled={updateMemory.isPending}>
                  <Save className="h-4 w-4" aria-hidden="true" />
                  Save tags
                </Button>
              </CardContent>
            </Card>

            <Card>
              <CardHeader>
                <CardTitle className="flex items-center gap-1.5">
                  <span>Importance override</span>
                  <HelpTooltip label="Importance override">Manual operator override for the memory's priority. Use sparingly for rules, decisions, or high-value context.</HelpTooltip>
                </CardTitle>
              </CardHeader>
              <CardContent className="grid gap-4">
                <label className="grid gap-2 text-sm text-ink/70">
                  <span className="flex justify-between text-xs font-medium uppercase text-ink/45">
                    <span>Score</span>
                    <span>{importanceDraft.toFixed(2)}</span>
                  </span>
                  <input
                    data-testid="importance-slider"
                    type="range"
                    min="0"
                    max="1"
                    step="0.01"
                    value={importanceDraft}
                    onChange={(event) => setImportanceDraft(Number(event.target.value))}
                    className="accent-accent"
                  />
                </label>
                {importanceError ? <InlineError title="Invalid importance" message={importanceError} /> : null}
                <Button type="button" data-testid="importance-save-button" onClick={saveImportance} disabled={updateMemory.isPending}>
                  <Save className="h-4 w-4" aria-hidden="true" />
                  Save override
                </Button>
              </CardContent>
            </Card>
          </section>

          <section className="grid gap-4 xl:grid-cols-2">
            <Card data-testid="version-history-panel">
              <CardHeader>
                <CardTitle className="flex items-center gap-1.5">
                  <span>Version History</span>
                  <HelpTooltip label="Version History">How this memory changed across edits, merges, and overrides. Versions are recorded for semantic memories.</HelpTooltip>
                </CardTitle>
              </CardHeader>
              <CardContent>
                {historyQuery.isLoading ? <Skeleton className="h-32 w-full" /> : null}
                {historyQuery.isError ? <InlineError title="Version history unavailable" message={errorMessage(historyQuery.error)} /> : null}
                {historyQuery.data && historyQuery.data.length > 0 ? (
                  <VersionTimeline versions={historyQuery.data} />
                ) : null}
                {historyQuery.data && historyQuery.data.length === 0 ? (
                  <EmptyState
                    title="No versions recorded"
                    message={memory.memory_type === "semantic"
                      ? "Version snapshots will appear here when this semantic memory is edited or merged."
                      : "Episodic memories are immutable; version history starts after promotion to semantic memory."}
                  />
                ) : null}
              </CardContent>
            </Card>
            <Card>
              <CardHeader>
                <CardTitle className="flex items-center gap-1.5">
                  <span>Retrieval Traces</span>
                  <HelpTooltip label="Retrieval Traces">Future operator view for when this memory was considered, included, or excluded during retrieval packing.</HelpTooltip>
                </CardTitle>
              </CardHeader>
              <CardContent>
                <EmptyState title="Retrieval Traces" message="Available in M6" />
              </CardContent>
            </Card>
          </section>
        </>
      ) : null}
    </div>
  );
}

function VersionTimeline({ versions }: { versions: MemoryVersion[] }) {
  const ordered = [...versions].sort((left, right) => right.version - left.version);

  return (
    <ol className="grid gap-2" data-testid="version-timeline">
      {ordered.map((version) => (
        <li key={version.id} className="grid gap-1 rounded-md border border-line bg-soft p-3">
          <div className="flex flex-wrap items-center gap-2">
            <Badge variant="accent">v{version.version}</Badge>
            <span className="text-xs text-ink/55">{formatDateTime(version.created_at)}</span>
            <span className="text-xs text-ink/55">by {version.edited_by}</span>
            <span className="ml-auto font-mono text-xs text-ink/65">importance {formatScore(version.importance_score)}</span>
          </div>
          <p className="text-sm leading-5 text-ink/80">{version.content}</p>
          {version.tags.length > 0 ? (
            <div className="flex flex-wrap gap-1.5">
              {version.tags.map((tag) => (
                <Badge key={tag} variant="gray">{tag}</Badge>
              ))}
            </div>
          ) : null}
        </li>
      ))}
    </ol>
  );
}

function ProvenanceTree({ graph }: { graph: ProvenanceGraph }) {
  const nodesById = new Map(graph.nodes.map((node) => [node.id, node]));
  const incoming = new Set(graph.edges.map((edge) => edge.to));
  const children = new Map<string, Array<{ id: string; edgeType: string }>>();
  graph.edges.forEach((edge) => {
    const current = children.get(edge.from) ?? [];
    current.push({ id: edge.to, edgeType: edge.edge_type });
    children.set(edge.from, current);
  });

  const roots = graph.nodes.filter((node) => !incoming.has(node.id));
  const renderRoots = roots.length > 0 ? roots : graph.nodes.filter((node) => node.id === graph.root_id);

  if (graph.nodes.length <= 1 && graph.edges.length === 0) {
    return <EmptyState title="Lineage is quiet" message="No source, promotion, merge, or access links are attached to this memory yet." />;
  }

  return (
    <div className="grid gap-2">
      {renderRoots.map((node) => (
        <ProvenanceBranch key={node.id} nodeId={node.id} nodesById={nodesById} children={children} depth={0} />
      ))}
    </div>
  );
}

function ProvenanceBranch({
  nodeId,
  nodesById,
  children,
  depth,
  edgeType,
  path = new Set<string>(),
}: {
  nodeId: string;
  nodesById: Map<string, ProvenanceNode>;
  children: Map<string, Array<{ id: string; edgeType: string }>>;
  depth: number;
  edgeType?: string;
  path?: Set<string>;
}) {
  const node = nodesById.get(nodeId);
  if (!node || path.has(nodeId)) {
    return null;
  }

  const nextPath = new Set(path).add(nodeId);
  const childNodes = children.get(nodeId) ?? [];

  return (
    <div className="grid gap-2">
      <div
        data-testid={provenanceNodeTestId(node.id)}
        className="grid gap-2 rounded-md border border-line bg-soft p-3 sm:grid-cols-[auto_1fr_auto] sm:items-center"
        style={{ marginLeft: depth * 18 }}
      >
        <div className="flex h-9 w-9 items-center justify-center rounded-md border border-line bg-white text-accent-strong">
          <ProvenanceIcon type={node.node_type} />
        </div>
        <div className="min-w-0">
          <div className="flex flex-wrap items-center gap-2">
            <p className="font-medium text-ink">{node.title}</p>
            <Badge variant={nodeBadge(node.node_type)}>{node.node_type.replace("_", " ")}</Badge>
            {edgeType ? <Badge variant="muted">{edgeLabel(edgeType)}</Badge> : null}
          </div>
          {node.subtitle ? <p className="mt-1 truncate text-sm text-ink/65">{node.subtitle}</p> : null}
        </div>
        <span className="text-xs text-ink/55">{formatDateTime(node.timestamp)}</span>
      </div>
      {childNodes.map((child) => (
        <ProvenanceBranch
          key={`${nodeId}:${child.id}:${child.edgeType}`}
          nodeId={child.id}
          nodesById={nodesById}
          children={children}
          depth={depth + 1}
          edgeType={child.edgeType}
          path={nextPath}
        />
      ))}
    </div>
  );
}

function ProvenanceIcon({ type }: { type: string }) {
  if (type === "raw_event") {
    return <GitCommit className="h-4 w-4" aria-hidden="true" />;
  }
  if (type === "merge") {
    return <GitMerge className="h-4 w-4" aria-hidden="true" />;
  }
  if (type === "access") {
    return <Clock3 className="h-4 w-4" aria-hidden="true" />;
  }
  return <Database className="h-4 w-4" aria-hidden="true" />;
}

function nodeBadge(type: string): "accent" | "blue" | "green" | "purple" | "muted" {
  if (type === "raw_event") {
    return "blue";
  }
  if (type === "merge") {
    return "purple";
  }
  if (type === "access") {
    return "green";
  }
  return "accent";
}

function edgeLabel(edgeType: string): string {
  if (edgeType === "created_from") {
    return "created";
  }
  if (edgeType === "promoted_to") {
    return "promoted";
  }
  if (edgeType === "merged_into") {
    return "merged";
  }
  if (edgeType === "accessed_as") {
    return "accessed";
  }
  return edgeType.replace("_", " ");
}

function provenanceNodeTestId(id: string): string {
  return `provenance-node-${id.replace(/[^a-zA-Z0-9_-]/g, "-")}`;
}

function ScoreLine({ label, value, helpText }: { label: string; value: string; helpText?: string }) {
  return (
    <div className="flex items-center justify-between rounded-md border border-line bg-soft px-3 py-2">
      <span className="text-sm text-ink/65">{helpText ? <InfoLabel label={label} tooltip={helpText} /> : label}</span>
      <span className="font-mono text-sm font-semibold">{value}</span>
    </div>
  );
}

function RelevanceMeter({ score }: { score: number }) {
  const clamped = Math.min(Math.max(score, 0), 1);
  const percentage = Math.round(clamped * 100);

  return (
    <div className="grid gap-2 rounded-md border border-line bg-soft px-3 py-2">
      <div className="flex items-center justify-between">
        <span className="text-sm text-ink/65">
          <InfoLabel label="Relevance" tooltip="Current retrieval relevance score for this memory in contexts where the backend provides one." />
        </span>
        <span className="font-mono text-sm font-semibold">{percentage}%</span>
      </div>
      <div className="h-2 overflow-hidden rounded-full bg-white">
        <div className="h-full rounded-full bg-accent" style={{ width: `${percentage}%` }} />
      </div>
    </div>
  );
}

function ScopeField({ label, value, helpText }: { label: string; value: string; helpText: string }) {
  return (
    <div className="min-w-0 rounded-md border border-line bg-soft p-3">
      <p className="text-xs font-medium uppercase text-ink/45">
        <InfoLabel label={label} tooltip={helpText} />
      </p>
      <p className="mt-1 break-all font-mono text-xs text-ink/75">{value}</p>
    </div>
  );
}

function TooltipBadge({ children, tooltip, variant }: { children: React.ReactNode; tooltip: React.ReactNode; variant: React.ComponentProps<typeof Badge>["variant"] }) {
  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <Badge variant={variant} tabIndex={0} className="focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent">
          {children}
        </Badge>
      </TooltipTrigger>
      <TooltipContent>{tooltip}</TooltipContent>
    </Tooltip>
  );
}

function DetailSkeleton() {
  return (
    <div className="grid gap-4">
      <Skeleton className="h-9 w-52" />
      <Skeleton className="h-64 w-full" />
      <div className="grid gap-4 xl:grid-cols-2">
        <Skeleton className="h-44 w-full" />
        <Skeleton className="h-44 w-full" />
      </div>
    </div>
  );
}

function normalizeScope(memory: MemoryUnit | undefined, workspaceId: string): Required<MemoryScope> {
  const scope = memory?.scope;
  const record = typeof scope === "object" && scope !== null && !Array.isArray(scope) ? scope : {};

  return {
    workspace_id: stringField(record, "workspace_id") ?? memory?.workspace_id ?? workspaceId,
    agent_id: nullableStringField(record, "agent_id"),
    user_id: nullableStringField(record, "user_id"),
    repo: nullableStringField(record, "repo"),
  };
}

function stringField(record: object, key: string): string | undefined {
  const value = (record as Record<string, unknown>)[key];
  return typeof value === "string" ? value : undefined;
}

function nullableStringField(record: object, key: string): string | null {
  return stringField(record, key) ?? null;
}

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : "Memory detail could not be loaded.";
}
