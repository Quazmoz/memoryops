import {
  Check,
  Copy,
  Download,
  Edit3,
  FileCode,
  History,
  Loader2,
  Plus,
  RotateCcw,
  Search,
  Trash2,
  X,
} from "lucide-react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useEffect, useMemo, useState, type FormEvent, type ReactNode } from "react";

import {
  createAgentResource,
  deleteAgentResource,
  getAgentResource,
  listAgentResources,
  listAgentResourceVersions,
  rollbackAgentResource,
  updateAgentResource,
  type AgentResource,
  type AgentResourceAssistant,
  type AgentResourceKind,
  type AgentResourceSummary,
  type AgentResourceVersion,
  type CreateAgentResourcePayload,
} from "../api/agentResources";
import type { JsonValue } from "../api/types";
import { EmptyState } from "../components/EmptyState";
import { InlineError } from "../components/InlineError";
import { Badge } from "../components/ui/badge";
import { Button } from "../components/ui/button";
import { Input } from "../components/ui/input";
import { Skeleton } from "../components/ui/skeleton";
import { cn } from "../lib/utils";

const resourceNamePattern = /^[a-z][a-z0-9_-]{0,63}$/;

const defaultBodyTemplates: Record<AgentResourceKind, string> = {
  skill: `## Trigger

Use this skill when:
- The user asks for this workflow explicitly.
- The task matches the durable workflow described below.

## Execution Steps

1. Describe the first step the agent should take.
2. Add important safety checks or constraints.
3. Explain the expected outcome or handoff.

## Failure Handling

- Say what the agent should do when required tools, credentials, or context are unavailable.

## Output Expectations

- Describe the final response or artifact shape.`,
  agent: `## Role

Define the agent's responsibility, boundaries, and default posture.

## Operating Rules

1. List the checks this agent should run before acting.
2. Describe when it should ask for clarification.
3. Define what it should hand back to the user.

## Failure Handling

- Define fallback behavior when inputs, tools, or upstream systems are unavailable.

## Output

Describe the required review, plan, or handoff format.`,
  prompt: `## Prompt

Write the reusable prompt body here.

## Inputs

- Describe each input variable.

## Output

Describe the desired response shape and any validation requirements.`,
  instruction: `## Instruction

State the reusable rule or operating constraint.

## Applies When

- Describe the situations where this instruction should be active.

## Failure Handling

- Explain what to do when the instruction conflicts with user intent, policy, or available evidence.`,
};

const resourceKinds: AgentResourceKind[] = ["skill", "agent", "prompt", "instruction"];
const allAssistants: AgentResourceAssistant[] = ["generic", "openai", "claude", "gemini"];
const skillAssistants: AgentResourceAssistant[] = ["claude", "gemini"];

type KindFilter = "all" | AgentResourceKind;
type AssistantFilter = "all" | AgentResourceAssistant;

type ResourceDraft = {
  kind: AgentResourceKind;
  assistant: AgentResourceAssistant;
  name: string;
  title: string;
  description: string;
  body: string;
  metadataText: string;
  change_note: string;
};

type SelectedResource = {
  kind: AgentResourceKind;
  assistant: AgentResourceAssistant;
  name: string;
};

type FormErrors = Partial<Record<keyof ResourceDraft, string>>;

export function AgentLibraryView() {
  const queryClient = useQueryClient();
  const [searchQuery, setSearchQuery] = useState("");
  const [selectedKind, setSelectedKind] = useState<KindFilter>("all");
  const [selectedAssistant, setSelectedAssistant] = useState<AssistantFilter>("all");
  const [selectedResource, setSelectedResource] = useState<SelectedResource | null>(null);
  const [copied, setCopied] = useState(false);
  const [drawerOpen, setDrawerOpen] = useState(false);
  const [editingResource, setEditingResource] = useState<SelectedResource | null>(null);
  const [draft, setDraft] = useState<ResourceDraft>(() => createEmptyDraft("skill"));
  const [initialDraft, setInitialDraft] = useState<ResourceDraft | null>(null);
  const [errors, setErrors] = useState<FormErrors>({});
  const [rollbackVersion, setRollbackVersion] = useState<number | null>(null);
  const [comparisonVersions, setComparisonVersions] = useState<number[]>([]);

  // Clear comparison selection whenever selected resource changes
  useEffect(() => {
    setComparisonVersions([]);
  }, [selectedResource]);

  const resourcesQuery = useQuery({
    queryKey: agentResourcesKey(selectedKind === "all" ? undefined : selectedKind),
    queryFn: () => listAgentResources(selectedKind === "all" ? {} : { kind: selectedKind }),
  });

  const resourceQuery = useQuery({
    queryKey: agentResourceContentKey(selectedResource),
    queryFn: () =>
      getAgentResource(selectedResource!.kind, selectedResource!.assistant, selectedResource!.name),
    enabled: selectedResource !== null,
  });

  const versionsQuery = useQuery({
    queryKey: agentResourceVersionsKey(selectedResource),
    queryFn: () =>
      listAgentResourceVersions(
        selectedResource!.kind,
        selectedResource!.assistant,
        selectedResource!.name,
      ),
    enabled: selectedResource !== null,
  });

  const createMutation = useMutation({
    mutationKey: ["agent-resources", "create"],
    mutationFn: (payload: CreateAgentResourcePayload) => createAgentResource(payload),
    onSuccess: (resource) => {
      finishSave(resource);
    },
  });

  const updateMutation = useMutation({
    mutationKey: ["agent-resources", "update"],
    mutationFn: ({
      resource,
      payload,
    }: {
      resource: SelectedResource;
      payload: Pick<CreateAgentResourcePayload, "title" | "description" | "body" | "metadata" | "change_note">;
    }) => updateAgentResource(resource.kind, resource.assistant, resource.name, payload),
    onSuccess: (resource) => {
      finishSave(resource);
    },
  });

  const rollbackMutation = useMutation({
    mutationKey: ["agent-resources", "rollback"],
    mutationFn: ({
      resource,
      version,
      changeNote,
    }: {
      resource: SelectedResource;
      version: number;
      changeNote?: string;
    }) => rollbackAgentResource(resource.kind, resource.assistant, resource.name, version, changeNote),
    onSuccess: (resource) => {
      setRollbackVersion(null);
      finishSave(resource, { keepDrawerOpen: false });
    },
  });

  const deleteMutation = useMutation({
    mutationKey: ["agent-resources", "delete"],
    mutationFn: (resource: SelectedResource) =>
      deleteAgentResource(resource.kind, resource.assistant, resource.name),
    onSuccess: async (_, resource) => {
      if (selectedResource && sameResource(selectedResource, resource)) {
        setSelectedResource(null);
      }
      await queryClient.invalidateQueries({ queryKey: ["agent-resources"] });
    },
  });

  const resources = resourcesQuery.data ?? [];
  const filteredResources = useMemo(() => {
    const search = searchQuery.trim().toLowerCase();
    return resources.filter((resource) => {
      const matchAssistant = selectedAssistant === "all" || resource.assistant === selectedAssistant;
      const metadataText = JSON.stringify(resource.metadata ?? {}).toLowerCase();
      const matchSearch =
        search.length === 0 ||
        resource.title.toLowerCase().includes(search) ||
        resource.description.toLowerCase().includes(search) ||
        resource.name.toLowerCase().includes(search) ||
        resource.filename.toLowerCase().includes(search) ||
        resourcePath(resource).toLowerCase().includes(search) ||
        metadataText.includes(search);
      return matchAssistant && matchSearch;
    });
  }, [resources, searchQuery, selectedAssistant]);

  const selectedResourceMeta = useMemo(() => {
    if (!selectedResource) return null;
    return resources.find((resource) => sameResource(resource, selectedResource)) ?? null;
  }, [resources, selectedResource]);

  const formPending = createMutation.isPending || updateMutation.isPending;
  const selectedContent = resourceQuery.data;
  const hasUnsavedChanges = drawerOpen && initialDraft !== null && draftSignature(draft) !== draftSignature(initialDraft);

  useEffect(() => {
    if (!hasUnsavedChanges) return undefined;
    const handleBeforeUnload = (event: BeforeUnloadEvent) => {
      event.preventDefault();
      event.returnValue = "";
    };
    window.addEventListener("beforeunload", handleBeforeUnload);
    return () => window.removeEventListener("beforeunload", handleBeforeUnload);
  }, [hasUnsavedChanges]);

  const leftComparedVersion = useMemo(() => {
    if (comparisonVersions.length === 0) return null;
    return versionsQuery.data?.find((v) => v.version === comparisonVersions[0]) ?? null;
  }, [comparisonVersions, versionsQuery.data]);

  const rightComparedVersion = useMemo(() => {
    if (comparisonVersions.length < 2) return null;
    return versionsQuery.data?.find((v) => v.version === comparisonVersions[1]) ?? null;
  }, [comparisonVersions, versionsQuery.data]);

  const comparisonDiffEntries = useMemo(() => {
    if (!leftComparedVersion || !rightComparedVersion) return [];
    return buildAgentResourceVersionDiffEntries(leftComparedVersion, rightComparedVersion);
  }, [leftComparedVersion, rightComparedVersion]);

  function finishSave(resource: AgentResource, options: { keepDrawerOpen?: boolean } = {}) {
    const selected = pickSelected(resource);
    setSelectedResource(selected);
    setSelectedKind(resource.kind);
    setComparisonVersions([]);
    if (selectedAssistant !== "all" && selectedAssistant !== resource.assistant) {
      setSelectedAssistant("all");
    }
    if (!options.keepDrawerOpen) {
      closeDrawer({ force: true });
    }
    void queryClient.invalidateQueries({ queryKey: ["agent-resources"] });
    void queryClient.invalidateQueries({ queryKey: agentResourceContentKey(selected) });
    void queryClient.invalidateQueries({ queryKey: agentResourceVersionsKey(selected) });
  }

  function openCreateDrawer(kind: AgentResourceKind = selectedKind === "all" ? "skill" : selectedKind) {
    const nextDraft = createEmptyDraft(kind);
    setEditingResource(null);
    setDraft(nextDraft);
    setInitialDraft(nextDraft);
    setErrors({});
    setDrawerOpen(true);
  }

  function openEditDrawer() {
    if (!selectedResource || !selectedContent) return;
    setEditingResource(selectedResource);
    const nextDraft = {
      kind: selectedResource.kind,
      assistant: selectedResource.assistant,
      name: selectedResource.name,
      title: selectedContent.title,
      description: selectedContent.description,
      body: selectedContent.body || defaultBodyTemplates[selectedResource.kind],
      metadataText: stringifyMetadata(selectedContent.metadata),
      change_note: "",
    };
    setDraft(nextDraft);
    setInitialDraft(nextDraft);
    setErrors({});
    setDrawerOpen(true);
  }

  function closeDrawer(options: { force?: boolean } = {}) {
    if (!options.force && hasUnsavedChanges && !window.confirm("Discard unsaved Agent Library changes?")) {
      return;
    }
    setDrawerOpen(false);
    setEditingResource(null);
    setInitialDraft(null);
    setErrors({});
  }

  function updateDraft(field: keyof ResourceDraft, value: string) {
    setDraft((current) => {
      if (field === "kind") {
        const nextKind = value as AgentResourceKind;
        const assistant = current.kind === nextKind ? current.assistant : defaultAssistant(nextKind);
        return {
          ...current,
          kind: nextKind,
          assistant,
          body: current.body || defaultBodyTemplates[nextKind],
        };
      }
      return { ...current, [field]: value };
    });
    setErrors((current) => ({ ...current, [field]: undefined }));
  }

  function submitResource(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const parsed = validateDraft(draft, Boolean(editingResource));
    setErrors(parsed.errors);
    if (!parsed.payload) return;

    if (editingResource) {
      const updatePayload: Pick<CreateAgentResourcePayload, "title" | "description" | "body" | "metadata" | "change_note"> = {
        title: parsed.payload.title,
        description: parsed.payload.description,
        body: parsed.payload.body,
      };
      if (parsed.payload.metadata) {
        updatePayload.metadata = parsed.payload.metadata;
      }
      if (parsed.payload.change_note) {
        updatePayload.change_note = parsed.payload.change_note;
      }
      updateMutation.mutate({
        resource: editingResource,
        payload: updatePayload,
      });
      return;
    }

    createMutation.mutate(parsed.payload);
  }

  const handleCopy = async () => {
    if (!selectedContent?.content) return;
    try {
      await navigator.clipboard.writeText(selectedContent.content);
      setCopied(true);
      window.setTimeout(() => setCopied(false), 2000);
    } catch (error) {
      console.error("Failed to copy agent resource markdown", error);
    }
  };

  const handleDownload = () => {
    if (!selectedContent) return;
    const blob = new Blob([selectedContent.content], { type: "text/markdown;charset=utf-8;" });
    const url = URL.createObjectURL(blob);
    const link = document.createElement("a");
    link.href = url;
    link.setAttribute("download", selectedContent.filename);
    document.body.appendChild(link);
    link.click();
    document.body.removeChild(link);
    URL.revokeObjectURL(url);
  };

  const handleDelete = () => {
    if (!selectedResource || !selectedContent) return;
    const confirmed = window.confirm(
      `Delete ${selectedContent.title}? This permanently removes the current ${singularKindLabel(selectedContent.kind).toLowerCase()} and all version snapshots. Rollback history will not be available after deletion.`,
    );
    if (confirmed) {
      deleteMutation.mutate(selectedResource);
    }
  };

  const handleRollback = (version: number) => {
    if (!selectedResource || !selectedContent) return;
    const confirmed = window.confirm(
      `Restore ${selectedContent.title} to v${version}? This creates a new version and keeps the existing history intact.`,
    );
    if (!confirmed) return;
    const suggestedNote = `restore ${selectedContent.name} to v${version}`;
    const changeNote = window.prompt("Optional change note for the new rollback version:", suggestedNote);
    setRollbackVersion(version);
    const rollbackPayload: { resource: SelectedResource; version: number; changeNote?: string } = {
      resource: selectedResource,
      version,
    };
    const trimmedChangeNote = changeNote?.trim();
    if (trimmedChangeNote) {
      rollbackPayload.changeNote = trimmedChangeNote;
    }
    rollbackMutation.mutate(rollbackPayload);
  };

  return (
    <div className="mx-auto grid max-w-7xl gap-5">
      <header className="flex flex-col gap-4 sm:flex-row sm:items-center sm:justify-between">
        <div>
          <p className="text-sm font-medium text-accent-strong">Agent resource library</p>
          <h1 className="mt-1 font-sans text-2xl font-semibold tracking-normal text-ink">Agent Library</h1>
        </div>
        <Button type="button" onClick={() => openCreateDrawer()}>
          <Plus className="h-4 w-4" aria-hidden="true" />
          Add Resource
        </Button>
      </header>

      {resourcesQuery.isError ? <InlineError message={errorMessage(resourcesQuery.error)} /> : null}
      {resourceQuery.isError ? <InlineError message="Failed to load the selected resource." /> : null}
      {createMutation.isError ? <InlineError title="Resource could not be created" message={errorMessage(createMutation.error)} /> : null}
      {updateMutation.isError ? <InlineError title="Resource could not be updated" message={errorMessage(updateMutation.error)} /> : null}
      {rollbackMutation.isError ? <InlineError title="Rollback failed" message={errorMessage(rollbackMutation.error)} /> : null}
      {deleteMutation.isError ? <InlineError title="Delete failed" message={errorMessage(deleteMutation.error)} /> : null}

      <div className="grid items-start gap-6 xl:grid-cols-[360px_1fr]">
        <aside className="flex flex-col gap-4 rounded-lg border border-line bg-white p-4 shadow-sm">
          <div className="relative">
            <Search className="absolute left-3 top-1/2 h-4 w-4 -translate-y-1/2 text-ink/40" aria-hidden="true" />
            <Input
              type="text"
              placeholder="Search library..."
              value={searchQuery}
              onChange={(event) => setSearchQuery(event.target.value)}
              className="border-line bg-soft/30 pl-9 text-sm focus:border-accent"
            />
          </div>

          <div className="grid grid-cols-2 gap-1 rounded-md bg-soft p-1 text-sm">
            <FilterTab active={selectedKind === "all"} onClick={() => setSelectedKind("all")}>All</FilterTab>
            {resourceKinds.map((kind) => (
              <FilterTab key={kind} active={selectedKind === kind} onClick={() => setSelectedKind(kind)}>
                {kindLabel(kind)}
              </FilterTab>
            ))}
          </div>

          <select
            value={selectedAssistant}
            onChange={(event) => setSelectedAssistant(event.target.value as AssistantFilter)}
            className="h-10 rounded-md border border-line bg-white px-3 text-sm text-ink outline-none focus:border-accent focus:ring-2 focus:ring-accent/20"
            aria-label="Filter target"
          >
            <option value="all">All targets</option>
            {allAssistants.map((assistant) => (
              <option key={assistant} value={assistant}>
                {assistantLabel(assistant)}
              </option>
            ))}
          </select>

          <div className="flex max-h-[620px] flex-col gap-2 overflow-y-auto pr-1 thin-scrollbar">
            {resourcesQuery.isLoading ? (
              <div className="flex flex-col gap-3">
                <Skeleton className="h-24 w-full" />
                <Skeleton className="h-24 w-full" />
                <Skeleton className="h-24 w-full" />
              </div>
            ) : null}

            {!resourcesQuery.isLoading && filteredResources.length === 0 ? (
              <div className="rounded-lg border border-dashed border-line bg-soft/20 px-4 py-8 text-center text-sm text-ink/55">
                No matching resources found.
              </div>
            ) : null}

            {!resourcesQuery.isLoading && filteredResources.map((resource) => {
              const isSelected = selectedResource ? sameResource(resource, selectedResource) : false;
              return (
                <button
                  key={`${resource.kind}-${resource.assistant}-${resource.name}`}
                  type="button"
                  onClick={() => setSelectedResource(pickSelected(resource))}
                  className={cn(
                    "flex w-full flex-col gap-2 rounded-lg border p-3.5 text-left transition-all duration-200 hover:border-accent/40",
                    isSelected
                      ? "border-accent bg-accent/5 ring-1 ring-accent"
                      : "border-line bg-white hover:bg-soft/40",
                  )}
                >
                  <div className="flex items-start justify-between gap-2">
                    <div className="min-w-0">
                      <span className="block truncate text-sm font-semibold text-ink">{resource.title}</span>
                      <span className="mt-1 block truncate font-mono text-[11px] text-ink/45">
                        {resourcePath(resource)}
                      </span>
                    </div>
                    <div className="flex shrink-0 flex-col items-end gap-1">
                      <KindBadge kind={resource.kind} />
                      <Badge variant="muted" className="px-1.5 py-0 text-[10px] font-medium">
                        v{resource.version}
                      </Badge>
                    </div>
                  </div>
                  <p className="line-clamp-2 text-xs leading-relaxed text-ink/65">{resource.description}</p>
                  <div className="flex flex-wrap items-center gap-2 text-[11px] font-medium text-ink/45">
                    <span>{assistantLabel(resource.assistant)}</span>
                    <span>Updated {formatDate(resource.updated_at)}</span>
                    <ResourceSourceBadge metadata={resource.metadata} />
                  </div>
                </button>
              );
            })}
          </div>
        </aside>

        <section className="grid min-h-[560px] overflow-hidden rounded-lg border border-line bg-white shadow-sm lg:grid-cols-[minmax(0,1fr)_280px]">
          {!selectedResource ? (
            <div className="flex flex-col items-center justify-center gap-4 p-8 lg:col-span-2">
              <EmptyState title="Select a resource" message="Choose an item from the library to inspect its content and version history." />
              <Button type="button" variant="secondary" onClick={() => openCreateDrawer()}>
                <Plus className="h-4 w-4" aria-hidden="true" />
                Create Resource
              </Button>
            </div>
          ) : (
            <>
              <div className="flex min-w-0 flex-col">
                <div className="flex flex-wrap items-center justify-between gap-3 border-b border-line bg-soft/20 px-5 py-4">
                  <div className="flex min-w-0 items-center gap-2">
                    <FileCode className="h-5 w-5 shrink-0 text-accent" aria-hidden="true" />
                    <div className="min-w-0">
                      <h2 className="truncate text-base font-semibold leading-none text-ink">
                        {selectedContent?.title || selectedResourceMeta?.title || selectedResource.name}
                      </h2>
                      <span className="mt-1 block truncate font-mono text-xs text-ink/50">
                        {selectedContent ? resourcePath(selectedContent) : selectedResource.name}
                      </span>
                    </div>
                  </div>

                  <div className="flex flex-wrap items-center gap-2">
                    <Button
                      type="button"
                      variant="secondary"
                      size="sm"
                      onClick={openEditDrawer}
                      disabled={resourceQuery.isLoading || resourceQuery.isError}
                    >
                      <Edit3 className="h-3.5 w-3.5" aria-hidden="true" />
                      Edit
                    </Button>
                    <Button
                      type="button"
                      variant="secondary"
                      size="sm"
                      onClick={handleCopy}
                      disabled={resourceQuery.isLoading || resourceQuery.isError}
                    >
                      {copied ? <Check className="h-3.5 w-3.5 text-green-600" aria-hidden="true" /> : <Copy className="h-3.5 w-3.5" aria-hidden="true" />}
                      {copied ? "Copied" : "Copy"}
                    </Button>
                    <Button
                      type="button"
                      size="sm"
                      onClick={handleDownload}
                      disabled={resourceQuery.isLoading || resourceQuery.isError}
                    >
                      <Download className="h-3.5 w-3.5" aria-hidden="true" />
                      Download
                    </Button>
                    <Button
                      type="button"
                      variant="destructive"
                      size="sm"
                      onClick={handleDelete}
                      disabled={deleteMutation.isPending || resourceQuery.isLoading || resourceQuery.isError}
                    >
                      <Trash2 className="h-3.5 w-3.5" aria-hidden="true" />
                      Delete
                    </Button>
                  </div>
                </div>

                <div className="thin-scrollbar max-h-[720px] flex-1 overflow-y-auto p-6">
                  {resourceQuery.isLoading ? (
                    <div className="space-y-4">
                      <Skeleton className="h-8 w-3/4" />
                      <Skeleton className="h-4 w-full" />
                      <Skeleton className="h-4 w-5/6" />
                      <Skeleton className="h-40 w-full" />
                    </div>
                  ) : null}

                  {!resourceQuery.isLoading && selectedContent ? (
                    leftComparedVersion && rightComparedVersion ? (
                      <div className="grid gap-4 select-text">
                        <div className="flex items-center justify-between border-b border-line pb-3">
                          <div>
                            <h3 className="text-base font-semibold text-ink">Version Comparison</h3>
                            <p className="text-xs text-ink/55">
                              Comparing v{leftComparedVersion.version} to v{rightComparedVersion.version}
                            </p>
                          </div>
                          <Badge variant="accent">
                            {comparisonDiffEntries.filter((entry) => entry.changed).length} fields changed
                          </Badge>
                        </div>
                        <div className="grid gap-6 mt-2">
                          {comparisonDiffEntries.map((entry) => (
                            <section key={entry.key} className="rounded-lg border border-line bg-soft/15 p-4">
                              <div className="mb-3 flex items-center justify-between gap-2">
                                <h4 className="text-xs font-bold uppercase tracking-wider text-ink/60">{entry.label}</h4>
                                <Badge variant={entry.changed ? "purple" : "muted"}>
                                  {entry.changed ? "Changed" : "Identical"}
                                </Badge>
                              </div>
                              <div className="grid gap-4 md:grid-cols-2">
                                <DiffValueCard
                                  label={`v${leftComparedVersion.version}`}
                                  value={entry.before}
                                  code={entry.code}
                                />
                                <DiffValueCard
                                  label={`v${rightComparedVersion.version}`}
                                  value={entry.after}
                                  code={entry.code}
                                />
                              </div>
                            </section>
                          ))}
                        </div>
                      </div>
                    ) : (
                      <div className="markdown-body select-text">
                        <MarkdownRenderer content={selectedContent.content} />
                      </div>
                    )
                  ) : null}
                </div>
              </div>

              <aside className="border-t border-line bg-soft/20 p-4 lg:border-l lg:border-t-0">
                {selectedContent ? (
                  <div className="mb-5 rounded-md border border-line bg-white p-3 text-xs text-ink/65">
                    <div className="flex flex-wrap items-center gap-2">
                      <KindBadge kind={selectedContent.kind} />
                      <Badge variant="muted">{assistantLabel(selectedContent.assistant)}</Badge>
                      <ResourceSourceBadge metadata={selectedContent.metadata} />
                    </div>
                    <dl className="mt-3 grid gap-2">
                      <div>
                        <dt className="font-medium uppercase text-ink/40">Path</dt>
                        <dd className="mt-1 break-all font-mono text-[11px] text-ink/70">{resourcePath(selectedContent)}</dd>
                      </div>
                      <div>
                        <dt className="font-medium uppercase text-ink/40">Updated</dt>
                        <dd className="mt-1">{formatDate(selectedContent.updated_at)}</dd>
                      </div>
                      <div>
                        <dt className="font-medium uppercase text-ink/40">Metadata</dt>
                        <dd className="thin-scrollbar mt-1 max-h-28 overflow-auto rounded border border-line bg-soft/30 p-2 font-mono text-[11px] text-ink/70">
                          {stringifyMetadata(selectedContent.metadata)}
                        </dd>
                      </div>
                    </dl>
                  </div>
                ) : null}

                <div className="flex flex-col gap-2">
                  <div className="flex items-center justify-between gap-2">
                    <div className="flex items-center gap-2 text-sm font-semibold text-ink">
                      <History className="h-4 w-4 text-accent" aria-hidden="true" />
                      Versions
                    </div>
                    {selectedContent ? <Badge variant="accent">v{selectedContent.version}</Badge> : null}
                  </div>

                  {versionsQuery.data && versionsQuery.data.length > 0 ? (
                    <div className="flex flex-wrap items-center gap-2 text-xs text-ink/55 mb-1 mt-0.5">
                      {comparisonVersions.length === 0 ? (
                        <span>Select two versions to compare.</span>
                      ) : (
                        <span>
                          Comparing: {comparisonVersions.map((v) => `v${v}`).join(" vs ")}
                        </span>
                      )}
                      {comparisonVersions.length > 0 ? (
                        <Button
                          type="button"
                          variant="ghost"
                          size="sm"
                          className="h-auto p-0 text-accent hover:underline"
                          onClick={() => setComparisonVersions([])}
                        >
                          Clear
                        </Button>
                      ) : null}
                    </div>
                  ) : null}
                </div>

                <div className="mt-2 grid gap-2">
                  {versionsQuery.isLoading ? (
                    <>
                      <Skeleton className="h-16 w-full" />
                      <Skeleton className="h-16 w-full" />
                    </>
                  ) : null}

                  {versionsQuery.data?.map((version) => {
                    const isCurrent = selectedContent?.version === version.version;
                    const isSelectedForCompare = comparisonVersions.includes(version.version);
                    return (
                      <div key={version.id} className="rounded-md border border-line bg-white p-3">
                        <div className="flex items-center justify-between gap-2">
                          <span className="text-sm font-semibold text-ink">v{version.version}</span>
                          {isCurrent ? <Badge variant="green">Current</Badge> : null}
                        </div>
                        <p className="mt-1 text-xs text-ink/55">{formatDate(version.created_at)}</p>
                        {version.change_note ? (
                          <p className="mt-2 line-clamp-2 text-xs leading-relaxed text-ink/70">{version.change_note}</p>
                        ) : null}
                        
                        <div className="mt-3 flex gap-1.5">
                          <Button
                            type="button"
                            variant={isSelectedForCompare ? "secondary" : "ghost"}
                            size="sm"
                            className="flex-1 text-xs"
                            onClick={() => {
                              setComparisonVersions((curr) => {
                                if (curr.includes(version.version)) {
                                  return curr.filter((val) => val !== version.version);
                                }
                                if (curr.length >= 2) {
                                  return [curr[1] as number, version.version];
                                }
                                return [...curr, version.version];
                              });
                            }}
                          >
                            {isSelectedForCompare ? "Selected" : "Compare"}
                          </Button>
                          {!isCurrent ? (
                            <Button
                              type="button"
                              variant="secondary"
                              size="sm"
                              className="flex-1 text-xs"
                              onClick={() => handleRollback(version.version)}
                              disabled={rollbackMutation.isPending}
                            >
                              {rollbackMutation.isPending && rollbackVersion === version.version ? (
                                <Loader2 className="h-3.5 w-3.5 animate-spin" aria-hidden="true" />
                              ) : (
                                <RotateCcw className="h-3.5 w-3.5" aria-hidden="true" />
                              )}
                              Restore
                            </Button>
                          ) : null}
                        </div>
                      </div>
                    );
                  })}
                </div>
              </aside>
            </>
          )}
        </section>
      </div>

      {drawerOpen ? (
        <div
          className="fixed inset-0 z-40 bg-ink/25"
          role="presentation"
          onMouseDown={(event) => event.target === event.currentTarget && closeDrawer()}
        >
          <aside
            className="ml-auto grid h-full w-full max-w-xl grid-rows-[auto_1fr] border-l border-line bg-white shadow-xl"
            role="dialog"
            aria-modal="true"
          >
            <div className="flex items-center justify-between border-b border-line px-5 py-4">
              <div>
                <h2 className="text-lg font-semibold text-ink">{editingResource ? "Edit Resource" : "Add Resource"}</h2>
                <p className="mt-1 text-sm text-ink/55">
                  {editingResource ? `${singularKindLabel(draft.kind)} / ${draft.name}` : singularKindLabel(draft.kind)}
                </p>
              </div>
              <Button type="button" variant="ghost" size="icon" aria-label="Close" onClick={() => closeDrawer()}>
                <X className="h-4 w-4" aria-hidden="true" />
              </Button>
            </div>

            <form className="grid content-start gap-4 overflow-y-auto p-5 thin-scrollbar" onSubmit={submitResource}>
              <div className="grid gap-4 sm:grid-cols-2">
                <Field label="Type" helpText="Resource category." error={errors.kind}>
                  <select
                    value={draft.kind}
                    onChange={(event) => updateDraft("kind", event.target.value)}
                    disabled={Boolean(editingResource)}
                    className="h-10 rounded-md border border-line bg-white px-3 text-sm text-ink outline-none focus:border-accent focus:ring-2 focus:ring-accent/20 disabled:opacity-60"
                  >
                    {resourceKinds.map((kind) => (
                      <option key={kind} value={kind}>{kindLabel(kind)}</option>
                    ))}
                  </select>
                </Field>
                <Field label="Target" helpText="Agent runtime or generic library." error={errors.assistant}>
                  <select
                    value={draft.assistant}
                    onChange={(event) => updateDraft("assistant", event.target.value)}
                    disabled={Boolean(editingResource)}
                    className="h-10 rounded-md border border-line bg-white px-3 text-sm text-ink outline-none focus:border-accent focus:ring-2 focus:ring-accent/20 disabled:opacity-60"
                  >
                    {allowedAssistants(draft.kind).map((assistant) => (
                      <option key={assistant} value={assistant}>{assistantLabel(assistant)}</option>
                    ))}
                  </select>
                </Field>
              </div>

              <Field label="Name" helpText="Lowercase letters, digits, underscores, or hyphens." error={errors.name}>
                <Input
                  value={draft.name}
                  onChange={(event) => updateDraft("name", event.target.value)}
                  disabled={Boolean(editingResource)}
                  placeholder="release_notes"
                />
              </Field>

              <Field label="Title" helpText="Shown in the library and generated markdown." error={errors.title}>
                <Input
                  value={draft.title}
                  onChange={(event) => updateDraft("title", event.target.value)}
                  placeholder={titlePlaceholder(draft.kind)}
                />
              </Field>

              <Field label="Description" helpText="Single-line summary." error={errors.description}>
                <Input
                  value={draft.description}
                  onChange={(event) => updateDraft("description", event.target.value)}
                  placeholder={descriptionPlaceholder(draft.kind)}
                />
              </Field>

              <Field label="Body" helpText="Markdown body saved into the version snapshot." error={errors.body}>
                <textarea
                  value={draft.body}
                  onChange={(event) => updateDraft("body", event.target.value)}
                  rows={18}
                  className="min-h-[340px] rounded-md border border-line bg-white px-3 py-2 font-mono text-sm outline-none focus:border-accent focus:ring-2 focus:ring-accent/20"
                />
              </Field>

              <Field label="Metadata" helpText="JSON object stored with this resource. Use it for source, default, owner, tags, or compatibility hints." error={errors.metadataText}>
                <textarea
                  value={draft.metadataText}
                  onChange={(event) => updateDraft("metadataText", event.target.value)}
                  rows={6}
                  className="min-h-32 rounded-md border border-line bg-white px-3 py-2 font-mono text-sm outline-none focus:border-accent focus:ring-2 focus:ring-accent/20"
                  spellCheck={false}
                />
              </Field>

              <Field label="Change Note" helpText={editingResource ? "Recommended for update history." : "Optional note for the first version."} error={errors.change_note}>
                <Input
                  value={draft.change_note}
                  onChange={(event) => updateDraft("change_note", event.target.value)}
                  placeholder={editingResource ? "Clarified trigger conditions" : "Initial version"}
                />
              </Field>

              <div className="rounded-md border border-line bg-soft/25 px-4 py-3 text-xs text-ink/65">
                Saved path:{" "}
                <code className="rounded bg-white px-1 py-0.5 font-mono text-[11px] text-ink">
                  {resourcePath({
                    kind: draft.kind,
                    assistant: draft.assistant,
                    name: draft.name || "your_resource",
                    filename: `${draft.name || "your_resource"}.md`,
                  })}
                </code>
              </div>

              <div className="flex justify-end gap-2 border-t border-line pt-4">
                <Button type="button" variant="secondary" onClick={() => closeDrawer()}>
                  Cancel
                </Button>
                <Button type="submit" disabled={formPending}>
                  {formPending ? <Loader2 className="h-4 w-4 animate-spin" aria-hidden="true" /> : <Check className="h-4 w-4" aria-hidden="true" />}
                  {editingResource ? "Save Resource" : "Create Resource"}
                </Button>
              </div>
            </form>
          </aside>
        </div>
      ) : null}
    </div>
  );
}

function FilterTab({
  active,
  onClick,
  children,
}: {
  active: boolean;
  onClick: () => void;
  children: ReactNode;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      className={cn(
        "rounded py-1.5 text-center font-medium transition-colors",
        active ? "bg-white text-ink shadow-sm" : "text-ink/60 hover:text-ink",
      )}
    >
      {children}
    </button>
  );
}

function Field({
  label,
  helpText,
  error,
  children,
}: {
  label: string;
  helpText: string;
  error?: string | undefined;
  children: ReactNode;
}) {
  return (
    <label className="grid gap-1 text-sm text-ink/70">
      <span className="text-xs font-medium uppercase text-ink/45">{label}</span>
      {children}
      <span className="text-xs text-ink/50">{helpText}</span>
      {error ? <span className="text-xs font-medium text-rust">{error}</span> : null}
    </label>
  );
}

function KindBadge({ kind }: { kind: AgentResourceKind }) {
  const variant =
    kind === "skill" ? "purple" : kind === "agent" ? "blue" : kind === "prompt" ? "teal" : "amber";
  return (
    <Badge variant={variant} className="px-1.5 py-0 text-[10px] font-medium">
      {singularKindLabel(kind)}
    </Badge>
  );
}

function createEmptyDraft(kind: AgentResourceKind): ResourceDraft {
  return {
    kind,
    assistant: defaultAssistant(kind),
    name: "",
    title: "",
    description: "",
    body: defaultBodyTemplates[kind],
    metadataText: "{}",
    change_note: "",
  };
}

function validateDraft(
  draft: ResourceDraft,
  editing: boolean,
): { payload?: CreateAgentResourcePayload; errors: FormErrors } {
  const errors: FormErrors = {};
  const name = draft.name.trim();
  const title = draft.title.trim();
  const description = draft.description.trim();
  const body = draft.body.trim();
  const metadataText = draft.metadataText.trim() || "{}";
  const changeNote = draft.change_note.trim();
  let metadata: Record<string, JsonValue> = {};

  if (!editing && !resourceNamePattern.test(name)) {
    errors.name = "Start with a lowercase letter and use only letters, digits, underscores, or hyphens.";
  }
  if (draft.kind === "skill" && !skillAssistants.includes(draft.assistant)) {
    errors.assistant = "Skills can target Claude or Gemini.";
  }
  if (title.length === 0 || title.length > 120 || hasLineBreak(title)) {
    errors.title = "Enter a single-line title up to 120 characters.";
  }
  if (description.length === 0 || description.length > 500 || hasLineBreak(description)) {
    errors.description = "Enter a single-line description up to 500 characters.";
  }
  if (body.length === 0 || body.length > 100_000) {
    errors.body = "Enter 1-100000 characters of markdown.";
  }
  try {
    const parsedMetadata = JSON.parse(metadataText) as unknown;
    if (!isMetadataRecord(parsedMetadata)) {
      errors.metadataText = "Metadata must be a JSON object.";
    } else {
      metadata = parsedMetadata;
    }
  } catch {
    errors.metadataText = "Enter valid JSON metadata.";
  }
  if (changeNote.length > 500) {
    errors.change_note = "Use 500 characters or fewer.";
  }

  if (Object.values(errors).some(Boolean)) {
    return { errors };
  }

  const payload: CreateAgentResourcePayload = {
    kind: draft.kind,
    assistant: draft.assistant,
    name,
    title,
    description,
    body,
    metadata,
  };
  if (changeNote) {
    payload.change_note = changeNote;
  }

  return { payload, errors };
}

function allowedAssistants(kind: AgentResourceKind): AgentResourceAssistant[] {
  return kind === "skill" ? skillAssistants : allAssistants;
}

function defaultAssistant(kind: AgentResourceKind): AgentResourceAssistant {
  return kind === "skill" ? "claude" : "generic";
}

function pickSelected(resource: Pick<AgentResourceSummary, "kind" | "assistant" | "name">): SelectedResource {
  return {
    kind: resource.kind,
    assistant: resource.assistant,
    name: resource.name,
  };
}

function sameResource(
  left: Pick<AgentResourceSummary, "kind" | "assistant" | "name">,
  right: Pick<AgentResourceSummary, "kind" | "assistant" | "name">,
): boolean {
  return left.kind === right.kind && left.assistant === right.assistant && left.name === right.name;
}

function kindLabel(kind: AgentResourceKind): string {
  switch (kind) {
    case "skill":
      return "Skills";
    case "agent":
      return "Agents";
    case "prompt":
      return "Prompts";
    case "instruction":
      return "Instructions";
  }
}

function singularKindLabel(kind: AgentResourceKind): string {
  switch (kind) {
    case "skill":
      return "Skill";
    case "agent":
      return "Agent";
    case "prompt":
      return "Prompt";
    case "instruction":
      return "Instruction";
  }
}

function assistantLabel(assistant: AgentResourceAssistant): string {
  switch (assistant) {
    case "generic":
      return "Generic";
    case "openai":
      return "OpenAI";
    case "claude":
      return "Claude";
    case "gemini":
      return "Gemini";
  }
}

function resourcePath(resource: Pick<AgentResourceSummary, "kind" | "assistant" | "name" | "filename">): string {
  if (resource.kind === "skill" && (resource.assistant === "claude" || resource.assistant === "gemini")) {
    return `.${resource.assistant}/skills/${resource.filename}`;
  }
  return `agent-library/${resource.assistant}/${kindLabel(resource.kind).toLowerCase()}/${resource.filename}`;
}

function ResourceSourceBadge({ metadata }: { metadata: Record<string, JsonValue> }) {
  const label = resourceSourceLabel(metadata);
  return (
    <Badge variant={label === "Custom" ? "muted" : "green"} className="px-1.5 py-0 text-[10px] font-medium">
      {label}
    </Badge>
  );
}

function resourceSourceLabel(metadata: Record<string, JsonValue>): string {
  if (metadata.default === true || metadata.seeded === true) {
    return "Default";
  }
  if (typeof metadata.source === "string" && metadata.source.trim().length > 0) {
    return metadata.source.trim();
  }
  return "Custom";
}

function stringifyMetadata(metadata: Record<string, JsonValue> | undefined): string {
  return JSON.stringify(metadata ?? {}, null, 2);
}

// Custom side-by-side Diff rendering card
function DiffValueCard({ label, value, code }: { label: string; value: string; code: boolean }) {
  return (
    <div className="grid gap-1 text-left">
      <span className="text-[11px] font-medium uppercase tracking-wide text-ink/45">{label}</span>
      {code ? (
        <pre className="thin-scrollbar max-h-60 overflow-auto rounded-md bg-zinc-950 px-3 py-2 font-mono text-xs text-zinc-100/90 whitespace-pre-wrap break-words">{value}</pre>
      ) : (
        <div className="rounded-md border border-line bg-white px-3 py-2 text-xs text-ink/75 whitespace-pre-wrap break-words">{value}</div>
      )}
    </div>
  );
}

interface AgentResourceVersionDiffEntry {
  key: string;
  label: string;
  before: string;
  after: string;
  changed: boolean;
  code: boolean;
}

function buildAgentResourceVersionDiffEntries(
  left: AgentResourceVersion,
  right: AgentResourceVersion,
): AgentResourceVersionDiffEntry[] {
  const entries: Array<Omit<AgentResourceVersionDiffEntry, "changed">> = [
    { key: "title", label: "Title", before: left.title, after: right.title, code: false },
    { key: "description", label: "Description", before: left.description, after: right.description, code: false },
    { key: "body", label: "Body", before: left.body, after: right.body, code: true },
  ];

  return entries.map((entry) => ({
    ...entry,
    changed: entry.before !== entry.after,
  }));
}

function isMetadataRecord(value: unknown): value is Record<string, JsonValue> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function draftSignature(draft: ResourceDraft): string {
  return JSON.stringify({
    ...draft,
    metadataText: normalizeMetadataText(draft.metadataText),
  });
}

function normalizeMetadataText(value: string): string {
  const trimmed = value.trim();
  if (!trimmed) return "{}";
  try {
    const parsed = JSON.parse(trimmed) as unknown;
    return isMetadataRecord(parsed) ? JSON.stringify(parsed) : trimmed;
  } catch {
    return trimmed;
  }
}

function titlePlaceholder(kind: AgentResourceKind): string {
  switch (kind) {
    case "skill":
      return "Release Notes Assistant";
    case "agent":
      return "Incident Coordinator";
    case "prompt":
      return "Release Brief Prompt";
    case "instruction":
      return "No Secrets in Logs";
  }
}

function descriptionPlaceholder(kind: AgentResourceKind): string {
  switch (kind) {
    case "skill":
      return "Summarises release notes and deployment changes.";
    case "agent":
      return "Coordinates incident response and status updates.";
    case "prompt":
      return "Drafts concise release notes from merged changes.";
    case "instruction":
      return "Prevents sensitive values from being printed or persisted.";
  }
}

function hasLineBreak(value: string): boolean {
  return value.includes("\n") || value.includes("\r");
}

function agentResourcesKey(kind?: AgentResourceKind) {
  return ["agent-resources", kind ?? "all"] as const;
}

function agentResourceContentKey(resource: SelectedResource | null) {
  return ["agent-resources", "content", resource?.kind ?? "", resource?.assistant ?? "", resource?.name ?? ""] as const;
}

function agentResourceVersionsKey(resource: SelectedResource | null) {
  return ["agent-resources", "versions", resource?.kind ?? "", resource?.assistant ?? "", resource?.name ?? ""] as const;
}

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : "Agent resources could not be loaded.";
}

function formatDate(value: string): string {
  return new Date(value).toLocaleString(undefined, {
    month: "short",
    day: "numeric",
    hour: "2-digit",
    minute: "2-digit",
  });
}

export const AgentSkillsView = AgentLibraryView;

interface MarkdownRendererProps {
  content: string;
}

function MarkdownRenderer({ content }: MarkdownRendererProps) {
  const lines = content.split("\n");
  const elements: ReactNode[] = [];
  let inCodeBlock = false;
  let codeBlockLanguage = "";
  let codeBlockLines: string[] = [];
  let listItems: ReactNode[] = [];
  let listOrdered = false;

  const flushList = () => {
    if (listItems.length === 0) return;
    const items = listItems;
    listItems = [];
    const key = `list-${elements.length}`;
    if (listOrdered) {
      elements.push(
        <ol key={key} className="my-2 ml-5 list-decimal space-y-1.5">
          {items}
        </ol>,
      );
    } else {
      elements.push(
        <ul key={key} className="my-2 ml-5 list-disc space-y-1.5">
          {items}
        </ul>,
      );
    }
  };

  const pushListItem = (ordered: boolean, node: ReactNode, key: string) => {
    if (listItems.length > 0 && listOrdered !== ordered) {
      flushList();
    }
    listOrdered = ordered;
    listItems.push(
      <li key={key} className="text-sm leading-relaxed text-ink/80">
        {node}
      </li>,
    );
  };

  for (let index = 0; index < lines.length; index += 1) {
    const line = lines[index];
    if (line === undefined) continue;

    if (line.trim().startsWith("```")) {
      if (inCodeBlock) {
        flushList();
        const codeText = codeBlockLines.join("\n");
        elements.push(
          <div key={`code-${index}`} className="my-4 overflow-hidden rounded-lg border border-line bg-zinc-900">
            <div className="flex items-center justify-between border-b border-zinc-700/50 bg-zinc-800/80 px-4 py-1.5 font-mono text-[11px] text-zinc-400">
              <span className="uppercase tracking-wider">{codeBlockLanguage || "code"}</span>
              <button
                type="button"
                onClick={() => navigator.clipboard.writeText(codeText)}
                className="flex items-center gap-1 transition-colors hover:text-white"
              >
                Copy
              </button>
            </div>
            <pre className="thin-scrollbar overflow-x-auto p-4 font-mono text-xs leading-relaxed text-zinc-100">
              <code>{codeText}</code>
            </pre>
          </div>,
        );
        codeBlockLines = [];
        inCodeBlock = false;
      } else {
        inCodeBlock = true;
        codeBlockLanguage = line.trim().substring(3).trim();
      }
      continue;
    }

    if (inCodeBlock) {
      codeBlockLines.push(line);
      continue;
    }

    if (line.startsWith("# ")) {
      flushList();
      elements.push(
        <h1 key={`h1-${index}`} className="first:mt-0 mb-4 mt-6 border-b border-line pb-2 font-sans text-2xl font-bold tracking-tight text-ink">
          {parseInlineMarkdown(line.substring(2))}
        </h1>,
      );
    } else if (line.startsWith("## ")) {
      flushList();
      elements.push(
        <h2 key={`h2-${index}`} className="mb-3 mt-6 border-b border-line/40 pb-1 font-sans text-lg font-semibold tracking-tight text-ink">
          {parseInlineMarkdown(line.substring(3))}
        </h2>,
      );
    } else if (line.startsWith("### ")) {
      flushList();
      elements.push(
        <h3 key={`h3-${index}`} className="mb-2 mt-4 font-sans text-sm font-semibold tracking-tight text-ink">
          {parseInlineMarkdown(line.substring(4))}
        </h3>,
      );
    } else if (line.trim().startsWith("- ") || line.trim().startsWith("* ")) {
      pushListItem(false, parseInlineMarkdown(line.trim().substring(2)), `li-${index}`);
    } else if (/^\d+\.\s/.test(line.trim())) {
      const match = line.trim().match(/^(\d+)\.\s(.*)/);
      if (match) {
        pushListItem(true, parseInlineMarkdown(match[2] ?? ""), `oli-${index}`);
      }
    } else if (line.trim() === "") {
      flushList();
      elements.push(<div key={`space-${index}`} className="h-2" />);
    } else {
      flushList();
      elements.push(
        <p key={`p-${index}`} className="my-2 font-sans text-sm leading-relaxed text-ink/80">
          {parseInlineMarkdown(line)}
        </p>,
      );
    }
  }

  flushList();

  return <div className="space-y-1">{elements}</div>;
}

function parseInlineMarkdown(text: string): ReactNode[] {
  const parts: ReactNode[] = [];
  let remaining = text;
  let keyIndex = 0;

  while (remaining.length > 0) {
    const boldIndex = remaining.indexOf("**");
    const codeIndex = remaining.indexOf("`");

    if (boldIndex === -1 && codeIndex === -1) {
      parts.push(<span key={keyIndex}>{remaining}</span>);
      break;
    }

    if (boldIndex !== -1 && (codeIndex === -1 || boldIndex < codeIndex)) {
      if (boldIndex > 0) {
        parts.push(<span key={keyIndex}>{remaining.substring(0, boldIndex)}</span>);
        keyIndex += 1;
      }
      const closingBoldIndex = remaining.indexOf("**", boldIndex + 2);
      if (closingBoldIndex !== -1) {
        const boldText = remaining.substring(boldIndex + 2, closingBoldIndex);
        parts.push(
          <strong key={keyIndex} className="font-semibold text-ink">
            {boldText}
          </strong>,
        );
        keyIndex += 1;
        remaining = remaining.substring(closingBoldIndex + 2);
      } else {
        parts.push(<span key={keyIndex}>{remaining.substring(boldIndex)}</span>);
        break;
      }
    } else {
      if (codeIndex > 0) {
        parts.push(<span key={keyIndex}>{remaining.substring(0, codeIndex)}</span>);
        keyIndex += 1;
      }
      const closingCodeIndex = remaining.indexOf("`", codeIndex + 1);
      if (closingCodeIndex !== -1) {
        const codeText = remaining.substring(codeIndex + 1, closingCodeIndex);
        parts.push(
          <code key={keyIndex} className="rounded border border-line/60 bg-soft px-1.5 py-0.5 font-mono text-[13px] font-semibold text-accent-strong">
            {codeText}
          </code>,
        );
        keyIndex += 1;
        remaining = remaining.substring(closingCodeIndex + 1);
      } else {
        parts.push(<span key={keyIndex}>{remaining.substring(codeIndex)}</span>);
        break;
      }
    }
  }

  return parts;
}
