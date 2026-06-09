import { Check, Copy, Download, Edit3, FileCode, Loader2, Plus, Search, X, History, RotateCcw } from "lucide-react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useMemo, useState, type FormEvent, type ReactNode } from "react";

import {
  createAgentSkill,
  getAgentSkill,
  listAgentSkills,
  updateAgentSkill,
  listAgentSkillVersions,
  rollbackAgentSkillVersion,
  type AgentSkillContent,
  type CreateAgentSkillPayload,
  type AgentSkillVersion,
} from "../api/agentSkills";
import { EmptyState } from "../components/EmptyState";
import { InlineError } from "../components/InlineError";
import { Badge } from "../components/ui/badge";
import { Button } from "../components/ui/button";
import { Input } from "../components/ui/input";
import { Skeleton } from "../components/ui/skeleton";
import { cn } from "../lib/utils";

const skillNamePattern = /^[a-z][a-z0-9_-]{0,63}$/;
const defaultInstructionsTemplate = `## Trigger

Use this skill when:
- The user asks for this workflow explicitly.

## Execution Steps

1. Describe the first step the agent should take.
2. Add any important safety checks or constraints.
3. Explain the expected outcome or handoff.`;

type SkillAssistant = "gemini" | "claude";
type AssistantFilter = "all" | SkillAssistant;

type SkillDraft = {
  assistant: SkillAssistant;
  name: string;
  title: string;
  description: string;
  instructions: string;
};

type FormErrors = Partial<Record<keyof SkillDraft, string>>;

export function AgentSkillsView() {
  const queryClient = useQueryClient();
  const [searchQuery, setSearchQuery] = useState("");
  const [selectedAssistant, setSelectedAssistant] = useState<AssistantFilter>("all");
  const [selectedSkill, setSelectedSkill] = useState<{ assistant: SkillAssistant; name: string } | null>(null);
  const [copied, setCopied] = useState(false);
  const [drawerOpen, setDrawerOpen] = useState(false);
  const [editingSkill, setEditingSkill] = useState<{ assistant: SkillAssistant; name: string } | null>(null);
  const [draft, setDraft] = useState<SkillDraft>(() => createEmptyDraft("claude"));
  const [errors, setErrors] = useState<FormErrors>({});
  const [detailTab, setDetailTab] = useState<"instructions" | "history">("instructions");
  const [comparisonVersions, setComparisonVersions] = useState<number[]>([]);
  const [confirmingRollback, setConfirmingRollback] = useState<number | null>(null);
  const [rollbackNote, setRollbackNote] = useState("");
  const [changeNote, setChangeNote] = useState("");

  const skillsQuery = useQuery({
    queryKey: agentSkillsKey(),
    queryFn: listAgentSkills,
  });

  const skillContentQuery = useQuery({
    queryKey: agentSkillContentKey(selectedSkill?.assistant, selectedSkill?.name),
    queryFn: () => getAgentSkill(selectedSkill!.assistant, selectedSkill!.name),
    enabled: selectedSkill !== null,
  });

  const versionsQuery = useQuery({
    queryKey: agentSkillVersionsKey(selectedSkill?.assistant, selectedSkill?.name),
    queryFn: () => listAgentSkillVersions(selectedSkill!.assistant, selectedSkill!.name),
    enabled: selectedSkill !== null && detailTab === "history",
  });

  const createMutation = useMutation({
    mutationKey: ["agent-skills", "create"],
    mutationFn: (payload: CreateAgentSkillPayload) => createAgentSkill(payload),
    onSuccess: (skill) => {
      finishSave(skill);
    },
  });

  const updateMutation = useMutation({
    mutationKey: ["agent-skills", "update"],
    mutationFn: ({ assistant, name, payload }: { assistant: SkillAssistant; name: string; payload: Omit<CreateAgentSkillPayload, "assistant" | "name"> }) =>
      updateAgentSkill(assistant, name, payload),
    onSuccess: (skill) => {
      finishSave(skill);
    },
  });

  const rollbackMutation = useMutation({
    mutationKey: ["agent-skills", "rollback"],
    mutationFn: ({ assistant, name, version, changeNote }: { assistant: SkillAssistant; name: string; version: number; changeNote?: string | undefined }) =>
      rollbackAgentSkillVersion(assistant, name, version, changeNote),
    onSuccess: (skill) => {
      setConfirmingRollback(null);
      setRollbackNote("");
      finishSave(skill);
    },
  });

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
    return buildAgentSkillVersionDiffEntries(leftComparedVersion, rightComparedVersion);
  }, [leftComparedVersion, rightComparedVersion]);

  const filteredSkills = useMemo(() => {
    const list = skillsQuery.data ?? [];
    return list.filter((skill) => {
      const matchAssistant = selectedAssistant === "all" || skill.assistant === selectedAssistant;
      const matchSearch =
        skill.title.toLowerCase().includes(searchQuery.toLowerCase()) ||
        skill.description.toLowerCase().includes(searchQuery.toLowerCase()) ||
        skill.name.toLowerCase().includes(searchQuery.toLowerCase());
      return matchAssistant && matchSearch;
    });
  }, [searchQuery, selectedAssistant, skillsQuery.data]);

  const selectedSkillMeta = useMemo(() => {
    if (!selectedSkill || !skillsQuery.data) return null;
    return skillsQuery.data.find(
      (skill) => skill.name === selectedSkill.name && skill.assistant === selectedSkill.assistant,
    ) ?? null;
  }, [selectedSkill, skillsQuery.data]);

  const formPending = createMutation.isPending || updateMutation.isPending;

  function finishSave(skill: AgentSkillContent) {
    setSelectedSkill({ assistant: skill.assistant, name: skill.name });
    setSelectedAssistant((current) =>
      current === "all" || current === skill.assistant ? current : skill.assistant,
    );
    closeDrawer();
    setDetailTab("instructions");
    setComparisonVersions([]);
    void queryClient.invalidateQueries({ queryKey: agentSkillsKey() });
    void queryClient.invalidateQueries({
      queryKey: agentSkillContentKey(skill.assistant, skill.name),
    });
    void queryClient.invalidateQueries({
      queryKey: agentSkillVersionsKey(skill.assistant, skill.name),
    });
  }

  function openCreateDrawer() {
    const assistant = selectedAssistant === "all" ? "claude" : selectedAssistant;
    setEditingSkill(null);
    setDraft(createEmptyDraft(assistant));
    setChangeNote("");
    setErrors({});
    setDrawerOpen(true);
  }

  function openEditDrawer() {
    if (!selectedSkill || !skillContentQuery.data) return;
    setEditingSkill(selectedSkill);
    setDraft({
      assistant: selectedSkill.assistant,
      name: selectedSkill.name,
      title: skillContentQuery.data.title,
      description: skillContentQuery.data.description,
      instructions: skillContentQuery.data.instructions || defaultInstructionsTemplate,
    });
    setChangeNote("");
    setErrors({});
    setDrawerOpen(true);
  }

  function closeDrawer() {
    setDrawerOpen(false);
    setEditingSkill(null);
    setErrors({});
  }

  function updateDraft(field: keyof SkillDraft, value: string) {
    setDraft((current) => ({ ...current, [field]: value }));
    setErrors((current) => ({ ...current, [field]: undefined }));
  }

  function submitSkill(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const parsed = validateDraft(draft);
    setErrors(parsed.errors);
    if (!parsed.payload) {
      return;
    }

    if (editingSkill) {
      updateMutation.mutate({
        assistant: editingSkill.assistant,
        name: editingSkill.name,
        payload: {
          title: parsed.payload.title,
          description: parsed.payload.description,
          instructions: parsed.payload.instructions,
          change_note: changeNote.trim() || undefined,
        },
      });
      return;
    }

    createMutation.mutate({
      ...parsed.payload,
      change_note: changeNote.trim() || undefined,
    });
  }

  const handleCopy = async () => {
    if (!skillContentQuery.data?.content) return;
    try {
      await navigator.clipboard.writeText(skillContentQuery.data.content);
      setCopied(true);
      window.setTimeout(() => setCopied(false), 2000);
    } catch (error) {
      console.error("Failed to copy agent skill markdown", error);
    }
  };

  const handleDownload = () => {
    if (!skillContentQuery.data) return;
    const { filename, content } = skillContentQuery.data;
    const blob = new Blob([content], { type: "text/markdown;charset=utf-8;" });
    const url = URL.createObjectURL(blob);
    const link = document.createElement("a");
    link.href = url;
    link.setAttribute("download", filename);
    document.body.appendChild(link);
    link.click();
    document.body.removeChild(link);
    URL.revokeObjectURL(url);
  };

  return (
    <div className="mx-auto grid max-w-7xl gap-5">
      <header className="flex flex-col gap-4 sm:flex-row sm:items-center sm:justify-between">
        <div>
          <p className="text-sm font-medium text-accent-strong">Agent skills library</p>
          <h1 className="mt-1 text-2xl font-semibold tracking-normal text-ink font-sans">Agent Skills</h1>
          <p className="mt-2 max-w-2xl text-sm text-ink/65">
            Create and maintain markdown skills for Claude and Gemini directly from the control center.
          </p>
        </div>
        <Button type="button" onClick={openCreateDrawer}>
          <Plus className="h-4 w-4" aria-hidden="true" />
          Add Skill
        </Button>
      </header>

      {skillsQuery.isError ? <InlineError message={errorMessage(skillsQuery.error)} /> : null}
      {createMutation.isError ? <InlineError title="Skill could not be created" message={errorMessage(createMutation.error)} /> : null}
      {updateMutation.isError ? <InlineError title="Skill could not be updated" message={errorMessage(updateMutation.error)} /> : null}

      <div className="grid items-start gap-6 lg:grid-cols-[340px_1fr]">
        <aside className="flex flex-col gap-4 rounded-lg border border-line bg-white p-4 shadow-sm">
          <div className="relative">
            <Search className="absolute left-3 top-1/2 h-4 w-4 -translate-y-1/2 text-ink/40" aria-hidden="true" />
            <Input
              type="text"
              placeholder="Search skills..."
              value={searchQuery}
              onChange={(event) => setSearchQuery(event.target.value)}
              className="border-line bg-soft/30 pl-9 text-sm focus:border-accent"
            />
          </div>

          <div className="flex rounded-md bg-soft p-1 text-sm">
            <FilterTab active={selectedAssistant === "all"} onClick={() => setSelectedAssistant("all")}>All</FilterTab>
            <FilterTab active={selectedAssistant === "gemini"} onClick={() => setSelectedAssistant("gemini")}>Gemini</FilterTab>
            <FilterTab active={selectedAssistant === "claude"} onClick={() => setSelectedAssistant("claude")}>Claude</FilterTab>
          </div>

          <div className="flex max-h-[560px] flex-col gap-2 overflow-y-auto pr-1 thin-scrollbar">
            {skillsQuery.isLoading ? (
              <div className="flex flex-col gap-3">
                <Skeleton className="h-20 w-full" />
                <Skeleton className="h-20 w-full" />
                <Skeleton className="h-20 w-full" />
              </div>
            ) : null}

            {!skillsQuery.isLoading && filteredSkills.length === 0 ? (
              <div className="rounded-lg border border-dashed border-line bg-soft/20 px-4 py-8 text-center text-sm text-ink/55">
                No matching agent skills found.
              </div>
            ) : null}

            {!skillsQuery.isLoading && filteredSkills.map((skill) => {
              const isSelected =
                selectedSkill?.name === skill.name &&
                selectedSkill?.assistant === skill.assistant;

              return (
                <button
                  key={`${skill.assistant}-${skill.name}`}
                  type="button"
                  onClick={() => {
                    setSelectedSkill({ assistant: skill.assistant, name: skill.name });
                    setDetailTab("instructions");
                    setComparisonVersions([]);
                    setConfirmingRollback(null);
                    setRollbackNote("");
                  }}
                  className={cn(
                    "flex w-full flex-col gap-2 rounded-lg border p-3.5 text-left transition-all duration-200 hover:border-accent/40",
                    isSelected
                      ? "border-accent bg-accent/5 ring-1 ring-accent"
                      : "border-line bg-white hover:bg-soft/40",
                  )}
                >
                  <div className="flex items-start justify-between gap-2">
                    <div className="min-w-0">
                      <span className="block truncate text-sm font-semibold text-ink">{skill.title}</span>
                      <span className="mt-1 block font-mono text-[11px] text-ink/45">{skill.name}</span>
                    </div>
                    <Badge
                      variant={skill.assistant === "gemini" ? "purple" : "rust"}
                      className="shrink-0 px-1.5 py-0 text-[10px] font-medium"
                    >
                      {skill.assistant === "gemini" ? "Gemini" : "Claude"}
                    </Badge>
                  </div>
                  <p className="line-clamp-2 text-xs leading-relaxed text-ink/65">{skill.description}</p>
                </button>
              );
            })}
          </div>
        </aside>

        <section className="flex min-h-[520px] flex-col overflow-hidden rounded-lg border border-line bg-white shadow-sm">
          {!selectedSkill ? (
            <div className="flex flex-1 flex-col items-center justify-center gap-4 p-8">
              <EmptyState
                title="Select a skill to preview"
                message="Choose an agent skill from the library on the left to read setup details, edit it, download it, or copy instructions for your AI agent."
              />
              <Button type="button" variant="secondary" onClick={openCreateDrawer}>
                <Plus className="h-4 w-4" aria-hidden="true" />
                Create Your First Skill
              </Button>
            </div>
          ) : (
            <div className="flex flex-1 flex-col">
              <div className="flex flex-wrap items-center justify-between gap-3 border-b border-line bg-soft/20 px-5 py-4">
                <div className="flex items-center gap-2">
                  <FileCode className="h-5 w-5 text-accent" />
                  <div>
                    <h2 className="text-base font-semibold leading-none text-ink">
                      {skillContentQuery.data?.title || selectedSkillMeta?.title || selectedSkill.name}
                    </h2>
                    <span className="mt-1 block text-xs font-mono text-ink/50">
                      {selectedSkill.assistant === "gemini" ? ".gemini" : ".claude"}/skills/{selectedSkill.name}.md
                    </span>
                  </div>
                </div>

                <div className="flex flex-wrap items-center gap-2">
                  <Button
                    type="button"
                    variant="secondary"
                    size="sm"
                    onClick={openEditDrawer}
                    disabled={skillContentQuery.isLoading || skillContentQuery.isError}
                  >
                    <Edit3 className="h-3.5 w-3.5" aria-hidden="true" />
                    Edit Skill
                  </Button>
                  <Button
                    type="button"
                    variant="secondary"
                    size="sm"
                    onClick={handleCopy}
                    disabled={skillContentQuery.isLoading || skillContentQuery.isError}
                  >
                    {copied ? (
                      <>
                        <Check className="h-3.5 w-3.5 text-green-600" aria-hidden="true" />
                        Copied!
                      </>
                    ) : (
                      <>
                        <Copy className="h-3.5 w-3.5" aria-hidden="true" />
                        Copy Markdown
                      </>
                    )}
                  </Button>
                  <Button
                    type="button"
                    size="sm"
                    onClick={handleDownload}
                    disabled={skillContentQuery.isLoading || skillContentQuery.isError}
                  >
                    <Download className="h-3.5 w-3.5" aria-hidden="true" />
                    Download File
                  </Button>
                </div>
              </div>

              <div className="flex items-start gap-3 border-b border-emerald-100/70 bg-emerald-50/60 px-5 py-3 text-xs text-emerald-950">
                <div className="mt-0.5 h-2 w-2 shrink-0 rounded-full bg-emerald-500" />
                <div>
                  <span className="font-semibold">Saved in this workspace.</span>{" "}
                  Compatible agents can load the markdown file from the assistant-specific skills directory shown above.
                </div>
              </div>

              <div className="flex border-b border-line bg-white px-5">
                <button
                  type="button"
                  onClick={() => setDetailTab("instructions")}
                  className={cn(
                    "border-b-2 px-4 py-2 text-sm font-medium transition-all duration-200 focus:outline-none",
                    detailTab === "instructions"
                      ? "border-accent text-accent"
                      : "border-transparent text-ink/65 hover:text-ink hover:border-line",
                  )}
                >
                  Instructions
                </button>
                <button
                  type="button"
                  onClick={() => setDetailTab("history")}
                  className={cn(
                    "border-b-2 px-4 py-2 text-sm font-medium transition-all duration-200 focus:outline-none",
                    detailTab === "history"
                      ? "border-accent text-accent"
                      : "border-transparent text-ink/65 hover:text-ink hover:border-line",
                  )}
                >
                  Version History
                </button>
              </div>

              <div className="thin-scrollbar flex-1 overflow-y-auto p-6" style={{ maxHeight: "680px" }}>
                {detailTab === "history" ? (
                  <div className="space-y-6">
                    <div className="flex items-center justify-between">
                      <h3 className="text-sm font-semibold text-ink">Version history</h3>
                      <span className="text-xs text-ink/55 text-accent-strong font-mono">Current: v{selectedSkillMeta?.version || 1}</span>
                    </div>

                    <div className="flex flex-wrap items-center gap-2 text-xs text-ink/55">
                      {comparisonVersions.length === 0 ? (
                        <span>Select up to two versions to compare.</span>
                      ) : (
                        <span>
                          Comparing queue: {comparisonVersions.map((v) => `v${v}`).join(" vs ")}
                        </span>
                      )}
                      {comparisonVersions.length > 0 ? (
                        <Button type="button" variant="ghost" size="sm" onClick={() => setComparisonVersions([])}>
                          Clear compare
                        </Button>
                      ) : null}
                    </div>

                    {versionsQuery.isLoading ? (
                      <div className="space-y-3">
                        <Skeleton className="h-10 w-full" />
                        <Skeleton className="h-10 w-full" />
                      </div>
                    ) : null}

                    {versionsQuery.isError ? (
                      <InlineError message="Failed to load version history." />
                    ) : null}

                    {rollbackMutation.isError ? (
                      <InlineError title="Rollback failed" message={errorMessage(rollbackMutation.error)} />
                    ) : null}

                    {versionsQuery.data && versionsQuery.data.length > 0 ? (
                      <div className="overflow-hidden rounded-md border border-line bg-white">
                        <table className="w-full border-collapse text-left text-sm">
                          <thead className="border-b border-line bg-soft/60 text-xs font-semibold uppercase text-ink/55">
                            <tr>
                              <th className="px-3 py-2">Version</th>
                              <th className="px-3 py-2">When</th>
                              <th className="px-3 py-2">By</th>
                              <th className="px-3 py-2">Change note</th>
                              <th className="px-3 py-2 text-right">Actions</th>
                            </tr>
                          </thead>
                          <tbody>
                            {versionsQuery.data.map((v) => {
                              const isCurrent = v.version === selectedSkillMeta?.version;
                              return (
                                <tr key={v.id} className="border-b border-line/70 last:border-b-0 align-top hover:bg-soft/20">
                                  <td className="px-3 py-2 font-mono text-xs text-ink">
                                    v{v.version}
                                    {isCurrent ? <span className="ml-1 text-[10px] uppercase font-bold text-accent-strong">current</span> : null}
                                  </td>
                                  <td className="px-3 py-2 text-xs text-ink/70">{new Date(v.created_at).toLocaleString()}</td>
                                  <td className="px-3 py-2 font-mono text-xs text-ink/60">{v.created_by ?? "—"}</td>
                                  <td className="px-3 py-2 text-xs text-ink/70">{v.change_note ?? <span className="text-ink/40">—</span>}</td>
                                  <td className="px-3 py-2 text-right">
                                    <div className="inline-flex flex-wrap justify-end gap-1.5">
                                      <Button
                                        type="button"
                                        variant={comparisonVersions.includes(v.version) ? "secondary" : "ghost"}
                                        size="sm"
                                        onClick={() => {
                                          setComparisonVersions((curr) => {
                                            if (curr.includes(v.version)) {
                                              return curr.filter((val) => val !== v.version);
                                            }
                                            if (curr.length >= 2) {
                                              return [curr[1] as number, v.version];
                                            }
                                            return [...curr, v.version];
                                          });
                                        }}
                                      >
                                        {comparisonVersions.includes(v.version) ? "Selected" : "Compare"}
                                      </Button>
                                      {!isCurrent ? (
                                        confirmingRollback === v.version ? (
                                          <div className="inline-grid gap-2 rounded-md border border-line bg-white p-2 text-left shadow-sm">
                                            <Input
                                              placeholder="Change note (optional)"
                                              value={rollbackNote}
                                              onChange={(e) => setRollbackNote(e.target.value)}
                                              className="h-8 text-xs"
                                            />
                                            <div className="flex justify-end gap-1.5">
                                              <Button
                                                type="button"
                                                variant="ghost"
                                                size="sm"
                                                onClick={() => {
                                                  setConfirmingRollback(null);
                                                  setRollbackNote("");
                                                }}
                                              >
                                                Cancel
                                              </Button>
                                              <Button
                                                type="button"
                                                size="sm"
                                                disabled={rollbackMutation.isPending}
                                                onClick={() =>
                                                  rollbackMutation.mutate({
                                                    assistant: selectedSkill.assistant,
                                                    name: selectedSkill.name,
                                                    version: v.version,
                                                    changeNote: rollbackNote.trim() || undefined,
                                                  })
                                                }
                                              >
                                                {rollbackMutation.isPending ? (
                                                  <Loader2 className="h-3 w-3 animate-spin" />
                                                ) : (
                                                  <RotateCcw className="h-3 w-3" />
                                                )}
                                                Confirm
                                              </Button>
                                            </div>
                                          </div>
                                        ) : (
                                          <Button
                                            type="button"
                                            variant="ghost"
                                            size="sm"
                                            onClick={() => {
                                              setConfirmingRollback(v.version);
                                              setRollbackNote("");
                                            }}
                                          >
                                            <RotateCcw className="h-3 w-3 mr-1" />
                                            Roll back
                                          </Button>
                                        )
                                      ) : null}
                                    </div>
                                  </td>
                                </tr>
                              );
                            })}
                          </tbody>
                        </table>
                      </div>
                    ) : null}

                    {versionsQuery.data && versionsQuery.data.length === 0 ? (
                      <p className="text-sm text-ink/60">No version history recorded yet.</p>
                    ) : null}

                    {leftComparedVersion && rightComparedVersion ? (
                      <div className="grid gap-3 rounded-md border border-line bg-white p-4">
                        <div className="flex flex-wrap items-center justify-between gap-2 border-b border-line pb-3">
                          <div>
                            <h4 className="text-sm font-semibold text-ink">Version diff</h4>
                            <p className="text-xs text-ink/55">
                              Comparing v{leftComparedVersion.version} to v{rightComparedVersion.version}
                            </p>
                          </div>
                          <Badge variant="accent">
                            {comparisonDiffEntries.filter((entry) => entry.changed).length} fields changed
                          </Badge>
                        </div>
                        <div className="grid gap-4 mt-2">
                          {comparisonDiffEntries.map((entry) => (
                            <section key={entry.key} className="rounded-md border border-line bg-soft/10 p-3">
                              <div className="mb-2 flex items-center justify-between gap-2">
                                <h5 className="text-xs font-semibold text-ink uppercase tracking-wider">{entry.label}</h5>
                                <Badge variant={entry.changed ? "purple" : "gray"}>
                                  {entry.changed ? "Changed" : "Same"}
                                </Badge>
                              </div>
                              <div className="grid gap-3 md:grid-cols-2">
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
                    ) : null}
                  </div>
                ) : (
                  <>
                    {skillContentQuery.isLoading ? (
                      <div className="space-y-4">
                        <Skeleton className="h-8 w-3/4" />
                        <Skeleton className="h-4 w-full" />
                        <Skeleton className="h-4 w-5/6" />
                        <Skeleton className="h-40 w-full" />
                      </div>
                    ) : null}

                    {skillContentQuery.isError ? (
                      <InlineError message="Failed to load the selected agent skill content." />
                    ) : null}

                    {!skillContentQuery.isLoading && skillContentQuery.data ? (
                      <div className="markdown-body select-text">
                        <MarkdownRenderer content={skillContentQuery.data.content} />
                      </div>
                    ) : null}
                  </>
                )}
              </div>
            </div>
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
                <h2 className="text-lg font-semibold text-ink">{editingSkill ? "Edit Skill" : "Add Skill"}</h2>
                <p className="mt-1 text-sm text-ink/55">
                  {editingSkill
                    ? "Update the markdown instructions saved in this repository."
                    : "Create a new markdown skill file for Claude or Gemini."}
                </p>
              </div>
              <Button type="button" variant="ghost" size="icon" aria-label="Close" onClick={closeDrawer}>
                <X className="h-4 w-4" aria-hidden="true" />
              </Button>
            </div>

            <form className="grid content-start gap-4 overflow-y-auto p-5 thin-scrollbar" onSubmit={submitSkill}>
              <div className="grid gap-4 sm:grid-cols-2">
                <Field label="Assistant" helpText="Choose which agent library should receive this markdown file." error={errors.assistant}>
                  <select
                    value={draft.assistant}
                    onChange={(event) => updateDraft("assistant", event.target.value)}
                    disabled={Boolean(editingSkill)}
                    className="h-10 rounded-md border border-line bg-white px-3 text-sm text-ink outline-none focus:border-accent focus:ring-2 focus:ring-accent/20 disabled:opacity-60"
                  >
                    <option value="claude">Claude</option>
                    <option value="gemini">Gemini</option>
                  </select>
                </Field>
                <Field label="File name" helpText="Use lowercase letters, digits, underscores, or hyphens." error={errors.name}>
                  <Input
                    value={draft.name}
                    onChange={(event) => updateDraft("name", event.target.value)}
                    disabled={Boolean(editingSkill)}
                    placeholder="release_notes"
                  />
                </Field>
              </div>

              <Field label="Title" helpText="Shown in the list and written into the markdown header." error={errors.title}>
                <Input
                  value={draft.title}
                  onChange={(event) => updateDraft("title", event.target.value)}
                  placeholder="Release Notes Assistant"
                />
              </Field>

              <Field label="Description" helpText="Short one-line summary for operators and future maintainers." error={errors.description}>
                <Input
                  value={draft.description}
                  onChange={(event) => updateDraft("description", event.target.value)}
                  placeholder="Summarises release notes and deployment changes."
                />
              </Field>

              <Field
                label="Instructions"
                helpText="Markdown body beneath the generated title and description. Include triggers, steps, and any constraints."
                error={errors.instructions}
              >
                <textarea
                  value={draft.instructions}
                  onChange={(event) => updateDraft("instructions", event.target.value)}
                  rows={18}
                  className="min-h-[340px] rounded-md border border-line bg-white px-3 py-2 font-mono text-sm outline-none focus:border-accent focus:ring-2 focus:ring-accent/20"
                />
              </Field>

              <Field
                label="Change note"
                helpText="Optional message summarizing the changes in this version snapshot."
                error={undefined}
              >
                <Input
                  value={changeNote}
                  onChange={(event) => setChangeNote(event.target.value)}
                  placeholder={editingSkill ? "e.g., Updated execution safety checklist" : "e.g., Initial version setup"}
                />
              </Field>

              <div className="rounded-md border border-line bg-soft/25 px-4 py-3 text-xs text-ink/60">
                Saved path:{" "}
                <code className="rounded bg-white px-1 py-0.5 font-mono text-[11px] text-ink">
                  .{draft.assistant}/skills/{draft.name || "your_skill"}.md
                </code>
              </div>

              <div className="flex justify-end gap-2 border-t border-line pt-4">
                <Button type="button" variant="secondary" onClick={closeDrawer}>
                  Cancel
                </Button>
                <Button type="submit" disabled={formPending}>
                  {formPending ? <Loader2 className="h-4 w-4 animate-spin" aria-hidden="true" /> : <Check className="h-4 w-4" aria-hidden="true" />}
                  {editingSkill ? "Save Skill" : "Create Skill"}
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
        "flex-1 rounded py-1.5 text-center font-medium transition-colors",
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

function createEmptyDraft(assistant: SkillAssistant): SkillDraft {
  return {
    assistant,
    name: "",
    title: "",
    description: "",
    instructions: defaultInstructionsTemplate,
  };
}

function validateDraft(draft: SkillDraft): { payload?: CreateAgentSkillPayload; errors: FormErrors } {
  const errors: FormErrors = {};
  const name = draft.name.trim();
  const title = draft.title.trim();
  const description = draft.description.trim();
  const instructions = draft.instructions.trim();

  if (!skillNamePattern.test(name)) {
    errors.name = "Start with a lowercase letter and use only letters, digits, underscores, or hyphens.";
  }
  if (title.length === 0 || title.length > 120 || hasLineBreak(title)) {
    errors.title = "Enter a single-line title up to 120 characters.";
  }
  if (description.length === 0 || description.length > 500 || hasLineBreak(description)) {
    errors.description = "Enter a single-line description up to 500 characters.";
  }
  if (instructions.length === 0 || instructions.length > 50_000) {
    errors.instructions = "Enter 1-50000 characters of markdown instructions.";
  }

  if (Object.values(errors).some(Boolean)) {
    return { errors };
  }

  return {
    payload: {
      assistant: draft.assistant,
      name,
      title,
      description,
      instructions,
    },
    errors,
  };
}

function hasLineBreak(value: string): boolean {
  return value.includes("\n") || value.includes("\r");
}

function agentSkillsKey() {
  return ["agent-skills"] as const;
}

function agentSkillContentKey(assistant?: SkillAssistant, name?: string) {
  return ["agent-skills", assistant ?? "", name ?? ""] as const;
}

function agentSkillVersionsKey(assistant?: SkillAssistant, name?: string) {
  return ["agent-skills", assistant ?? "", name ?? "", "versions"] as const;
}

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

interface AgentSkillVersionDiffEntry {
  key: string;
  label: string;
  before: string;
  after: string;
  changed: boolean;
  code: boolean;
}

function buildAgentSkillVersionDiffEntries(left: AgentSkillVersion, right: AgentSkillVersion): AgentSkillVersionDiffEntry[] {
  const entries: Array<Omit<AgentSkillVersionDiffEntry, "changed">> = [
    { key: "title", label: "Title", before: left.title, after: right.title, code: false },
    { key: "description", label: "Description", before: left.description, after: right.description, code: false },
    { key: "instructions", label: "Instructions", before: left.instructions, after: right.instructions, code: true },
  ];

  return entries.map((entry) => ({
    ...entry,
    changed: entry.before !== entry.after,
  }));
}

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : "Agent skills could not be loaded.";
}

interface MarkdownRendererProps {
  content: string;
}

function MarkdownRenderer({ content }: MarkdownRendererProps) {
  const lines = content.split("\n");
  const elements: ReactNode[] = [];
  let inCodeBlock = false;
  let codeBlockLanguage = "";
  let codeBlockLines: string[] = [];

  for (let index = 0; index < lines.length; index += 1) {
    const line = lines[index];
    if (line === undefined) continue;

    if (line.trim().startsWith("```")) {
      if (inCodeBlock) {
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
      elements.push(
        <h1 key={`h1-${index}`} className="first:mt-0 mb-4 mt-6 border-b border-line pb-2 font-sans text-2xl font-bold tracking-tight text-ink">
          {parseInlineMarkdown(line.substring(2))}
        </h1>,
      );
    } else if (line.startsWith("## ")) {
      elements.push(
        <h2 key={`h2-${index}`} className="mb-3 mt-6 border-b border-line/40 pb-1 font-sans text-lg font-semibold tracking-tight text-ink">
          {parseInlineMarkdown(line.substring(3))}
        </h2>,
      );
    } else if (line.startsWith("### ")) {
      elements.push(
        <h3 key={`h3-${index}`} className="mb-2 mt-4 font-sans text-sm font-semibold tracking-tight text-ink">
          {parseInlineMarkdown(line.substring(4))}
        </h3>,
      );
    } else if (line.trim().startsWith("- ") || line.trim().startsWith("* ")) {
      elements.push(
        <li key={`li-${index}`} className="my-1.5 ml-5 list-disc text-sm leading-relaxed text-ink/80">
          {parseInlineMarkdown(line.trim().substring(2))}
        </li>,
      );
    } else if (/^\d+\.\s/.test(line.trim())) {
      const match = line.trim().match(/^(\d+)\.\s(.*)/);
      if (match) {
        elements.push(
          <li key={`oli-${index}`} className="my-1.5 ml-5 list-decimal text-sm leading-relaxed text-ink/80">
            {parseInlineMarkdown(match[2] ?? "")}
          </li>,
        );
      }
    } else if (line.trim() === "") {
      elements.push(<div key={`space-${index}`} className="h-2" />);
    } else {
      elements.push(
        <p key={`p-${index}`} className="my-2 font-sans text-sm leading-relaxed text-ink/80">
          {parseInlineMarkdown(line)}
        </p>,
      );
    }
  }

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
