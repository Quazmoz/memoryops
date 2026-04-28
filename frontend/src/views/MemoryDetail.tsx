import { ArrowLeft, Check, Plus, Save, X } from "lucide-react";
import { useEffect, useMemo, useState, type FormEvent } from "react";
import { Link, useParams } from "react-router-dom";

import type { MemoryScope, MemoryUnit } from "../api/types";
import { EmptyState } from "../components/EmptyState";
import { EntityChip } from "../components/EntityChip";
import { InlineError } from "../components/InlineError";
import { Badge } from "../components/ui/badge";
import { Button } from "../components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "../components/ui/card";
import { Input } from "../components/ui/input";
import { Skeleton } from "../components/ui/skeleton";
import { useMemoryDetail, useUpdateMemory } from "../hooks/use-memory";
import { formatCount, formatDateTime, formatRelativeTime, formatScore } from "../lib/format";
import { validateImportanceScore } from "../lib/validation";
import { useAppStore } from "../store/app-store";

export function MemoryDetail() {
  const { id } = useParams<{ id?: string }>();
  const workspaceId = useAppStore((state) => state.workspaceId);
  const memoryQuery = useMemoryDetail(workspaceId, id);
  const updateMemory = useUpdateMemory(workspaceId);
  const memory = memoryQuery.data;
  const scope = useMemo(() => normalizeScope(memory, workspaceId), [memory, workspaceId]);
  const [draftTags, setDraftTags] = useState<string[]>([]);
  const [tagInput, setTagInput] = useState("");
  const [importanceDraft, setImportanceDraft] = useState(0);
  const [importanceError, setImportanceError] = useState<string | null>(null);

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
                <Badge variant={memory.memory_type === "semantic" ? "teal" : "rust"}>{memory.memory_type === "semantic" ? "Semantic" : "Episodic"}</Badge>
                {memory.importance_overridden ? <Badge variant="amber">overridden</Badge> : null}
                {memory.memory_type === "semantic" && memory.corroboration_count > 1 ? <Badge variant="purple">⬡ {memory.corroboration_count} sources</Badge> : null}
                <span className="text-sm text-ink/55">Updated {formatDateTime(memory.updated_at)}</span>
                {memory.promoted_at ? <span className="text-sm text-ink/55">Promoted {formatRelativeTime(memory.promoted_at)}</span> : null}
              </div>
              <h1 className="mt-3 text-2xl font-semibold tracking-normal text-ink">Memory Detail</h1>
            </div>
            <Button type="button" variant={memory.pinned ? "secondary" : "default"} data-testid="detail-pin-toggle" onClick={togglePinned} disabled={updateMemory.isPending}>
              <Check className="h-4 w-4" aria-hidden="true" />
              {memory.pinned ? "Pinned" : "Pin"}
            </Button>
          </header>

          {updateMemory.isError ? <InlineError message={errorMessage(updateMemory.error)} /> : null}

          <section className="grid gap-4 xl:grid-cols-[1fr_22rem]">
            <Card>
              <CardHeader>
                <CardTitle>Content</CardTitle>
              </CardHeader>
              <CardContent>
                <div className="whitespace-pre-wrap rounded-lg border border-line bg-soft p-4 text-sm leading-6 text-ink">{memory.content}</div>
              </CardContent>
            </Card>

            <Card>
              <CardHeader>
                <CardTitle>Scores</CardTitle>
              </CardHeader>
              <CardContent className="grid gap-3">
                <ScoreLine label="Importance" value={formatScore(memory.importance_score)} />
                <ScoreLine label="Decay" value={formatScore(memory.decay_score)} />
                <ScoreLine label="Access count" value={formatCount(memory.access_count)} />
                {memory.promoted_at ? <ScoreLine label="Promoted" value={formatRelativeTime(memory.promoted_at)} /> : null}
              </CardContent>
            </Card>
          </section>

          <section className="grid gap-4 xl:grid-cols-2">
            <Card>
              <CardHeader>
                <CardTitle>Entities</CardTitle>
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
                <CardTitle>Scope</CardTitle>
              </CardHeader>
              <CardContent className="grid gap-3 sm:grid-cols-2">
                <ScopeField label="workspace_id" value={scope.workspace_id} />
                <ScopeField label="agent_id" value={scope.agent_id ?? "workspace-wide"} />
                <ScopeField label="user_id" value={scope.user_id ?? "all users"} />
                <ScopeField label="repo" value={scope.repo ?? "all repos"} />
              </CardContent>
            </Card>
          </section>

          <section className="grid gap-4 xl:grid-cols-2">
            <Card>
              <CardHeader>
                <CardTitle>Tags</CardTitle>
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
                <CardTitle>Importance override</CardTitle>
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
            <EmptyState title="Version History" message="Available in M6" />
            <EmptyState title="Retrieval Traces" message="Available in M6" />
          </section>
        </>
      ) : null}
    </div>
  );
}

function ScoreLine({ label, value }: { label: string; value: string }) {
  return (
    <div className="flex items-center justify-between rounded-md border border-line bg-soft px-3 py-2">
      <span className="text-sm text-ink/65">{label}</span>
      <span className="font-mono text-sm font-semibold">{value}</span>
    </div>
  );
}

function ScopeField({ label, value }: { label: string; value: string }) {
  return (
    <div className="min-w-0 rounded-md border border-line bg-soft p-3">
      <p className="text-xs font-medium uppercase text-ink/45">{label}</p>
      <p className="mt-1 break-all font-mono text-xs text-ink/75">{value}</p>
    </div>
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