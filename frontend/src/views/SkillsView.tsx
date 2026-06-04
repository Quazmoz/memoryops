import { Check, Edit3, FlaskConical, History, Loader2, Play, Plus, RotateCcw, Trash2, X } from "lucide-react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Fragment, useEffect, useMemo, useState, type FormEvent } from "react";

import { createSkill, deleteSkill, listSkillVersions, listSkills, rollbackSkillVersion, testSkill, updateSkill, type CreateSkillPayload, type Skill, type SkillTestResponse, type SkillVersion } from "../api/skills";
import type { JsonValue } from "../api/types";
import { EmptyState } from "../components/EmptyState";
import { InlineError } from "../components/InlineError";
import { Badge } from "../components/ui/badge";
import { Button } from "../components/ui/button";
import { Input } from "../components/ui/input";
import { Skeleton } from "../components/ui/skeleton";
import { HelpTooltip, InfoLabel, Tooltip, TooltipContent, TooltipTrigger } from "../components/ui/tooltip";
import { previewText } from "../lib/format";
import { cn } from "../lib/utils";
import { useAppStore } from "../store/app-store";

const skillNamePattern = /^[a-z][a-z0-9_]{0,63}$/;
const emptyDraft = {
  name: "",
  description: "",
  endpoint_url: "https://",
  http_method: "POST",
  scope_visibility: "workspace" as NonNullable<CreateSkillPayload["scope_visibility"]>,
  auth_header: "",
  auth_secret: "",
  input_schema: "{}",
  output_schema: "{}",
};

type SkillDraft = typeof emptyDraft;
type FormErrors = Partial<Record<keyof SkillDraft, string>>;

export function SkillsView() {
  const workspaceId = useAppStore((state) => state.workspaceId);
  const apiKey = useAppStore((state) => state.apiKey);
  const queryClient = useQueryClient();
  const [drawerOpen, setDrawerOpen] = useState(false);
  const [editingSkill, setEditingSkill] = useState<Skill | null>(null);
  const [draft, setDraft] = useState<SkillDraft>(emptyDraft);
  const [errors, setErrors] = useState<FormErrors>({});
  const [confirmingDelete, setConfirmingDelete] = useState<string | null>(null);
  const [testingSkillName, setTestingSkillName] = useState<string | null>(null);
  const [testBody, setTestBody] = useState("");
  const [testResult, setTestResult] = useState<SkillTestResponse | null>(null);
  const [testError, setTestError] = useState<string | null>(null);
  const [historySkillName, setHistorySkillName] = useState<string | null>(null);
  const [rollbackNote, setRollbackNote] = useState("");
  const [confirmingRollback, setConfirmingRollback] = useState<number | null>(null);
  const [comparisonVersions, setComparisonVersions] = useState<number[]>([]);
  const hasAuth = workspaceId.trim().length > 0 && apiKey.trim().length > 0;

  const skillsQuery = useQuery({
    queryKey: skillsKey(workspaceId),
    queryFn: () => listSkills(workspaceId),
    enabled: hasAuth,
  });

  const createMutation = useMutation({
    mutationKey: ["workspace", workspaceId, "skills", "create"],
    mutationFn: (payload: CreateSkillPayload) => createSkill(workspaceId, payload),
    onSuccess: () => {
      resetDrawer();
      void queryClient.invalidateQueries({ queryKey: skillsKey(workspaceId) });
    },
  });

  const updateMutation = useMutation({
    mutationKey: ["workspace", workspaceId, "skills", "update"],
    mutationFn: ({ name, patch }: { name: string; patch: Partial<CreateSkillPayload> }) => updateSkill(workspaceId, name, patch),
    onMutate: async ({ name, patch }) => {
      await queryClient.cancelQueries({ queryKey: skillsKey(workspaceId) });
      const snapshot = queryClient.getQueryData<Skill[]>(skillsKey(workspaceId));
      queryClient.setQueryData<Skill[]>(skillsKey(workspaceId), (current) =>
        current?.map((skill) => (skill.name === name ? { ...skill, ...patch } as Skill : skill)),
      );
      return { snapshot };
    },
    onError: (_error, _variables, context) => {
      queryClient.setQueryData(skillsKey(workspaceId), context?.snapshot);
    },
    onSuccess: (skill) => {
      queryClient.setQueryData<Skill[]>(skillsKey(workspaceId), (current) => current?.map((item) => (item.id === skill.id ? skill : item)) ?? [skill]);
      if (editingSkill) {
        resetDrawer();
      }
    },
    onSettled: () => {
      void queryClient.invalidateQueries({ queryKey: skillsKey(workspaceId) });
    },
  });

  const deleteMutation = useMutation({
    mutationKey: ["workspace", workspaceId, "skills", "delete"],
    mutationFn: (name: string) => deleteSkill(workspaceId, name),
    onSuccess: (_result, name) => {
      setConfirmingDelete(null);
      queryClient.setQueryData<Skill[]>(skillsKey(workspaceId), (current) => current?.filter((skill) => skill.name !== name) ?? []);
    },
    onSettled: () => {
      void queryClient.invalidateQueries({ queryKey: skillsKey(workspaceId) });
    },
  });

  const testMutation = useMutation({
    mutationKey: ["workspace", workspaceId, "skills", "test"],
    mutationFn: ({ name, body }: { name: string; body: JsonValue }) => testSkill(workspaceId, name, { body }),
    onSuccess: (data) => {
      setTestResult(data);
      setTestError(null);
    },
    onError: (error) => {
      setTestError(error instanceof Error ? error.message : "Test request failed.");
      setTestResult(null);
    },
  });

  const versionsQuery = useQuery({
    queryKey: skillVersionsKey(workspaceId, historySkillName ?? ""),
    queryFn: () => listSkillVersions(workspaceId, historySkillName as string),
    enabled: hasAuth && historySkillName !== null,
  });

  const rollbackMutation = useMutation({
    mutationKey: ["workspace", workspaceId, "skills", "rollback"],
    mutationFn: ({ name, version, change_note }: { name: string; version: number; change_note?: string | undefined }) =>
      rollbackSkillVersion(workspaceId, name, version, change_note),
    onSuccess: (skill) => {
      setConfirmingRollback(null);
      setRollbackNote("");
      setComparisonVersions([]);
      queryClient.setQueryData<Skill[]>(skillsKey(workspaceId), (current) =>
        current?.map((item) => (item.id === skill.id ? skill : item)) ?? [skill],
      );
      void queryClient.invalidateQueries({ queryKey: skillVersionsKey(workspaceId, skill.name) });
      void queryClient.invalidateQueries({ queryKey: skillsKey(workspaceId) });
    },
  });

  const rows = useMemo(() => skillsQuery.data ?? [], [skillsQuery.data]);
  const comparedVersions = useMemo(
    () => comparisonVersions
      .map((version) => versionsQuery.data?.find((candidate) => candidate.version === version))
      .filter((candidate): candidate is SkillVersion => Boolean(candidate)),
    [comparisonVersions, versionsQuery.data],
  );
  const [leftComparedVersion, rightComparedVersion] = comparedVersions;
  const comparisonDiffEntries = leftComparedVersion && rightComparedVersion
    ? buildSkillVersionDiffEntries(leftComparedVersion, rightComparedVersion)
    : [];
  const formPending = createMutation.isPending || updateMutation.isPending;

  useEffect(() => {
    if (!drawerOpen) {
      setErrors({});
    }
  }, [drawerOpen]);

  function openCreateDrawer() {
    setEditingSkill(null);
    setDraft(emptyDraft);
    setErrors({});
    setDrawerOpen(true);
  }

  function openEditDrawer(skill: Skill) {
    setEditingSkill(skill);
    setDraft({
      name: skill.name,
      description: skill.description,
      endpoint_url: skill.endpoint_url,
      http_method: skill.http_method,
      scope_visibility: skill.scope_visibility,
      auth_header: skill.auth_header ?? "",
      auth_secret: "",
      input_schema: JSON.stringify(skill.input_schema ?? {}, null, 2),
      output_schema: JSON.stringify(skill.output_schema ?? {}, null, 2),
    });
    setErrors({});
    setDrawerOpen(true);
  }

  function resetDrawer() {
    setDrawerOpen(false);
    setEditingSkill(null);
    setDraft(emptyDraft);
    setErrors({});
  }

  function updateDraft(field: keyof SkillDraft, value: string) {
    setDraft((current) => ({ ...current, [field]: value }));
    setErrors((current) => ({ ...current, [field]: undefined }));
  }

  function submitSkill(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const parsed = validateDraft(draft, Boolean(editingSkill));
    setErrors(parsed.errors);
    if (!parsed.payload) {
      return;
    }

    if (editingSkill) {
      updateMutation.mutate({ name: editingSkill.name, patch: parsed.payload });
    } else {
      createMutation.mutate(parsed.payload as CreateSkillPayload);
    }
  }

  function toggleEnabled(skill: Skill) {
    updateMutation.mutate({ name: skill.name, patch: { enabled: !skill.enabled } });
  }

  function openTestPanel(skill: Skill) {
    if (testingSkillName === skill.name) {
      setTestingSkillName(null);
      return;
    }
    setTestingSkillName(skill.name);
    setHistorySkillName(null);
    setComparisonVersions([]);
    setTestBody(JSON.stringify(skill.input_schema ?? {}, null, 2));
    setTestResult(null);
    setTestError(null);
  }

  function openHistoryPanel(skill: Skill) {
    if (historySkillName === skill.name) {
      setHistorySkillName(null);
      setConfirmingRollback(null);
      setComparisonVersions([]);
      return;
    }
    setHistorySkillName(skill.name);
    setTestingSkillName(null);
    setRollbackNote("");
    setConfirmingRollback(null);
    setComparisonVersions([]);
  }

  function toggleVersionComparison(version: number) {
    setComparisonVersions((current) => {
      if (current.includes(version)) {
        return current.filter((value) => value !== version);
      }
      if (current.length === 2) {
        const latest = current[1];
        return latest === undefined ? [version] : [latest, version];
      }
      return [...current, version];
    });
  }

  function runTest(name: string) {
    let body: JsonValue;
    try {
      body = JSON.parse(testBody || "{}") as JsonValue;
    } catch {
      setTestError("Invalid JSON in request body.");
      return;
    }
    setTestError(null);
    testMutation.mutate({ name, body });
  }

  return (
    <div className="mx-auto grid max-w-7xl gap-5">
      <header className="flex flex-col gap-4 sm:flex-row sm:items-center sm:justify-between">
        <div>
          <p className="text-sm font-medium text-accent-strong">Agent tools</p>
          <h1 className="mt-1 text-2xl font-semibold tracking-normal text-ink">Skills</h1>
        </div>
        <Tooltip>
          <TooltipTrigger asChild>
            <Button type="button" data-testid="skill-add-button" onClick={openCreateDrawer} disabled={!hasAuth}>
              <Plus className="h-4 w-4" aria-hidden="true" />
              Add Skill
            </Button>
          </TooltipTrigger>
          <TooltipContent>HTTP tools agents can call to augment memory retrieval with live external data.</TooltipContent>
        </Tooltip>
      </header>

      {skillsQuery.isError ? <InlineError message={errorMessage(skillsQuery.error)} /> : null}
      {createMutation.isError ? <InlineError title="Skill could not be saved" message={errorMessage(createMutation.error)} /> : null}
      {updateMutation.isError ? <InlineError title="Skill update failed" message={errorMessage(updateMutation.error)} /> : null}
      {deleteMutation.isError ? <InlineError title="Skill delete failed" message={errorMessage(deleteMutation.error)} /> : null}

      {skillsQuery.isLoading ? <SkillsSkeleton /> : null}

      {!skillsQuery.isLoading && rows.length === 0 ? (
        <EmptyState title="No skills registered" message="No skills registered. Add your first HTTP Skill to extend agent retrieval." />
      ) : null}

      {rows.length > 0 ? (
        <section className="overflow-hidden rounded-lg border border-line bg-white">
          <div className="thin-scrollbar overflow-x-auto">
            <table className="w-full min-w-[920px] border-collapse text-left">
              <thead className="border-b border-line bg-soft/80 text-xs font-semibold uppercase text-ink/55">
                <tr>
                  <th className="px-4 py-3"><InfoLabel label="Name" tooltip="Machine-safe identifier. Use lowercase letters, numbers, and underscores." /></th>
                  <th className="px-4 py-3"><InfoLabel label="Description" tooltip="What the skill does so operators and agents understand when to call it." /></th>
                  <th className="px-4 py-3"><InfoLabel label="Method" tooltip="HTTP method MemoryOps will use when invoking the skill." /></th>
                  <th className="px-4 py-3"><InfoLabel label="URL" tooltip="HTTPS endpoint MemoryOps will call when this skill is invoked. Local, private, and network metadata URLs should be rejected by the backend." /></th>
                  <th className="px-4 py-3"><InfoLabel label="Enabled" tooltip="Whether agents may call this skill during retrieval and tool use." /></th>
                  <th className="px-4 py-3 text-right">Actions</th>
                </tr>
              </thead>
              <tbody>
                {rows.map((skill) => (
                  <Fragment key={skill.id}>
                  <tr data-testid={`skill-row-${skill.name}`} className="border-b border-line/80 last:border-b-0">
                    <td className="px-4 py-4 align-middle font-mono text-sm text-ink">
                      <span>{skill.name}</span>
                      <span
                        className="ml-2 inline-flex items-center rounded border border-line bg-soft px-1.5 py-0.5 text-[10px] font-semibold text-ink/60"
                        title={`Current version: ${skill.version}`}
                        data-testid={`skill-version-${skill.name}`}
                      >
                        v{skill.version}
                      </span>
                      <Badge variant={scopeBadgeVariant(skill.scope_visibility)} className="ml-2 align-middle">
                        {formatSkillScopeVisibility(skill.scope_visibility)}
                      </Badge>
                    </td>
                    <td className="max-w-[22rem] px-4 py-4 align-middle text-sm text-ink/70">{previewText(skill.description, 96)}</td>
                    <td className="px-4 py-4 align-middle">
                      <Badge variant="gray">{skill.http_method}</Badge>
                    </td>
                    <td className="max-w-[24rem] truncate px-4 py-4 align-middle font-mono text-xs text-ink/60">
                      <TooltipText value={skill.endpoint_url}>{skill.endpoint_url}</TooltipText>
                    </td>
                    <td className="px-4 py-4 align-middle">
                      <Tooltip>
                        <TooltipTrigger asChild>
                          <button
                            type="button"
                            data-testid={`skill-enabled-${skill.name}`}
                            className={toggleClass(skill.enabled)}
                            onClick={() => toggleEnabled(skill)}
                            disabled={updateMutation.isPending}
                            aria-pressed={skill.enabled}
                            aria-label={skill.enabled ? `Disable ${skill.name}` : `Enable ${skill.name}`}
                          >
                            <span className={cn("h-4 w-4 rounded-full bg-white shadow transition", skill.enabled ? "translate-x-5" : "translate-x-0")} />
                          </button>
                        </TooltipTrigger>
                        <TooltipContent>{skill.enabled ? "Agents can currently invoke this skill." : "Enable this skill so agents can invoke it."}</TooltipContent>
                      </Tooltip>
                    </td>
                    <td className="relative px-4 py-4 align-middle">
                      <div className="flex justify-end gap-2">
                        <Tooltip>
                          <TooltipTrigger asChild>
                            <Button
                              type="button"
                              variant="ghost"
                              size="icon"
                              data-testid={`skill-test-open-${skill.name}`}
                              aria-label={`Test ${skill.name}`}
                              aria-pressed={testingSkillName === skill.name}
                              onClick={() => openTestPanel(skill)}
                            >
                              <FlaskConical className="h-4 w-4" aria-hidden="true" />
                            </Button>
                          </TooltipTrigger>
                          <TooltipContent>Sends a live request to the skill endpoint using the provided test body.</TooltipContent>
                        </Tooltip>
                        <Tooltip>
                          <TooltipTrigger asChild>
                            <Button
                              type="button"
                              variant="ghost"
                              size="icon"
                              data-testid={`skill-history-open-${skill.name}`}
                              aria-label={`History for ${skill.name}`}
                              aria-pressed={historySkillName === skill.name}
                              onClick={() => openHistoryPanel(skill)}
                            >
                              <History className="h-4 w-4" aria-hidden="true" />
                            </Button>
                          </TooltipTrigger>
                          <TooltipContent>View past versions and roll back to a previous configuration.</TooltipContent>
                        </Tooltip>
                        <Tooltip>
                          <TooltipTrigger asChild>
                            <Button type="button" variant="ghost" size="icon" data-testid={`skill-edit-${skill.name}`} aria-label={`Edit ${skill.name}`} onClick={() => openEditDrawer(skill)}>
                              <Edit3 className="h-4 w-4" aria-hidden="true" />
                            </Button>
                          </TooltipTrigger>
                          <TooltipContent>Edit the saved skill configuration.</TooltipContent>
                        </Tooltip>
                        <Tooltip>
                          <TooltipTrigger asChild>
                            <Button type="button" variant="ghost" size="icon" data-testid={`skill-delete-${skill.name}`} aria-label={`Delete ${skill.name}`} onClick={() => setConfirmingDelete(skill.name)}>
                              <Trash2 className="h-4 w-4" aria-hidden="true" />
                            </Button>
                          </TooltipTrigger>
                          <TooltipContent>Delete this registered skill.</TooltipContent>
                        </Tooltip>
                      </div>
                      {confirmingDelete === skill.name ? (
                        <div className="absolute right-4 z-10 mt-2 w-64 rounded-lg border border-line bg-white p-3 text-sm shadow-lg">
                          <p className="font-medium text-ink">Delete {skill.name}?</p>
                          <div className="mt-3 flex justify-end gap-2">
                            <Button type="button" variant="ghost" size="sm" onClick={() => setConfirmingDelete(null)}>Cancel</Button>
                            <Button type="button" variant="destructive" size="sm" onClick={() => deleteMutation.mutate(skill.name)} disabled={deleteMutation.isPending}>
                              {deleteMutation.isPending ? <Loader2 className="h-3.5 w-3.5 animate-spin" aria-hidden="true" /> : <Trash2 className="h-3.5 w-3.5" aria-hidden="true" />}
                              Delete
                            </Button>
                          </div>
                        </div>
                      ) : null}
                    </td>
                  </tr>
                  {testingSkillName === skill.name ? (
                    <tr>
                      <td colSpan={6} className="border-b border-line/80 bg-soft/40 px-5 py-4">
                        <div className="grid max-w-3xl gap-4">
                          <div className="flex flex-wrap gap-6 text-sm">
                            <div>
                              <span className="text-xs font-medium uppercase text-ink/45"><InfoLabel label="Method" tooltip="HTTP method MemoryOps will use for this test request." /></span>
                              <p className="mt-0.5 font-mono text-ink">{skill.http_method}</p>
                            </div>
                            <div className="min-w-0 flex-1">
                              <span className="text-xs font-medium uppercase text-ink/45"><InfoLabel label="URL" tooltip="HTTPS endpoint MemoryOps will call when this skill is invoked." /></span>
                              <p className="mt-0.5 truncate font-mono text-xs text-ink/70">{skill.endpoint_url}</p>
                            </div>
                          </div>
                          <label className="grid gap-1">
                            <span className="text-xs font-medium uppercase text-ink/45"><InfoLabel label="Request body JSON" tooltip="Live request body sent to the skill endpoint during a test run." /></span>
                            <textarea
                              data-testid={`skill-test-body-${skill.name}`}
                              value={testBody}
                              onChange={(e) => setTestBody(e.target.value)}
                              rows={5}
                              className="rounded-md border border-line bg-white px-3 py-2 font-mono text-sm outline-none focus:border-accent focus:ring-2 focus:ring-accent/20"
                            />
                          </label>
                          {testError ? <p className="text-sm text-rust">{testError}</p> : null}
                          <div className="flex items-center gap-3">
                            <Tooltip>
                              <TooltipTrigger asChild>
                                <Button
                                  type="button"
                                  size="sm"
                                  data-testid={`skill-test-run-${skill.name}`}
                                  onClick={() => runTest(skill.name)}
                                  disabled={testMutation.isPending}
                                >
                                  {testMutation.isPending
                                    ? <Loader2 className="h-3.5 w-3.5 animate-spin" aria-hidden="true" />
                                    : <Play className="h-3.5 w-3.5" aria-hidden="true" />}
                                  Run test
                                </Button>
                              </TooltipTrigger>
                              <TooltipContent>Sends a live request to the skill endpoint using the provided test body.</TooltipContent>
                            </Tooltip>
                            {testResult ? (
                              <span className="text-sm text-ink/60">
                                <span className={statusColor(testResult.status)}>{testResult.status}</span>
                                {" · "}{testResult.latency_ms} ms
                              </span>
                            ) : null}
                          </div>
                          {testResult ? (
                            <div className="grid gap-1">
                              <span className="text-xs font-medium uppercase text-ink/45"><InfoLabel label="Response panel" tooltip="Raw response returned by the skill test request." /></span>
                              <pre
                                data-testid={`skill-test-response-${skill.name}`}
                                className="max-h-64 overflow-auto rounded-md bg-ink px-4 py-3 font-mono text-xs text-white/90"
                              >
                                {JSON.stringify(testResult.body, null, 2)}
                              </pre>
                            </div>
                          ) : null}
                        </div>
                      </td>
                    </tr>
                  ) : null}
                  {historySkillName === skill.name ? (
                    <tr>
                      <td colSpan={6} className="border-b border-line/80 bg-soft/40 px-5 py-4">
                        <div className="grid max-w-3xl gap-3" data-testid={`skill-history-${skill.name}`}>
                          <div className="flex items-center justify-between">
                            <h3 className="text-sm font-semibold text-ink">Version history</h3>
                            <span className="text-xs text-ink/55">Current: v{skill.version}</span>
                          </div>
                          <div className="flex flex-wrap items-center gap-2 text-xs text-ink/55">
                            {comparisonVersions.length === 0 ? (
                              <span>Select up to two versions to compare.</span>
                            ) : (
                              <span>
                                Comparing queue: {comparisonVersions.map((version) => `v${version}`).join(" vs ")}
                              </span>
                            )}
                            {comparisonVersions.length > 0 ? (
                              <Button type="button" variant="ghost" size="sm" onClick={() => setComparisonVersions([])}>
                                Clear compare
                              </Button>
                            ) : null}
                          </div>
                          {versionsQuery.isLoading ? <Skeleton className="h-24 w-full" /> : null}
                          {versionsQuery.isError ? <InlineError message={errorMessage(versionsQuery.error)} /> : null}
                          {rollbackMutation.isError ? <InlineError title="Rollback failed" message={errorMessage(rollbackMutation.error)} /> : null}
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
                                  {versionsQuery.data.map((v) => (
                                    <tr key={v.id} data-testid={`skill-version-row-${skill.name}-${v.version}`} className="border-b border-line/70 last:border-b-0 align-top">
                                      <td className="px-3 py-2 font-mono text-xs text-ink">v{v.version}{v.version === skill.version ? <span className="ml-1 text-[10px] uppercase text-accent-strong">current</span> : null}</td>
                                      <td className="px-3 py-2 text-xs text-ink/70">{new Date(v.created_at).toLocaleString()}</td>
                                      <td className="px-3 py-2 font-mono text-xs text-ink/60">{v.created_by ?? "—"}</td>
                                      <td className="px-3 py-2 text-xs text-ink/70">{v.change_note ?? <span className="text-ink/40">—</span>}</td>
                                      <td className="px-3 py-2 text-right">
                                        <div className="inline-flex flex-wrap justify-end gap-1.5">
                                          <Button
                                            type="button"
                                            variant={comparisonVersions.includes(v.version) ? "secondary" : "ghost"}
                                            size="sm"
                                            data-testid={`skill-compare-${skill.name}-${v.version}`}
                                            onClick={() => toggleVersionComparison(v.version)}
                                          >
                                            {comparisonVersions.includes(v.version) ? "Selected" : "Compare"}
                                          </Button>
                                          {v.version !== skill.version ? (
                                            confirmingRollback === v.version ? (
                                              <div className="inline-grid gap-2 rounded-md border border-line bg-white p-2 text-left shadow">
                                                <Input
                                                  data-testid={`skill-rollback-note-${skill.name}-${v.version}`}
                                                  placeholder="Change note (optional)"
                                                  value={rollbackNote}
                                                  onChange={(e) => setRollbackNote(e.target.value)}
                                                />
                                                <div className="flex justify-end gap-1.5">
                                                  <Button type="button" variant="ghost" size="sm" onClick={() => { setConfirmingRollback(null); setRollbackNote(""); }}>Cancel</Button>
                                                  <Button
                                                    type="button"
                                                    size="sm"
                                                    data-testid={`skill-rollback-confirm-${skill.name}-${v.version}`}
                                                    disabled={rollbackMutation.isPending}
                                                    onClick={() => rollbackMutation.mutate({ name: skill.name, version: v.version, change_note: rollbackNote.trim() || undefined })}
                                                  >
                                                    {rollbackMutation.isPending ? <Loader2 className="h-3.5 w-3.5 animate-spin" aria-hidden="true" /> : <RotateCcw className="h-3.5 w-3.5" aria-hidden="true" />}
                                                    Confirm rollback
                                                  </Button>
                                                </div>
                                              </div>
                                            ) : (
                                              <Button
                                                type="button"
                                                variant="ghost"
                                                size="sm"
                                                data-testid={`skill-rollback-${skill.name}-${v.version}`}
                                                onClick={() => { setConfirmingRollback(v.version); setRollbackNote(""); }}
                                              >
                                                <RotateCcw className="h-3.5 w-3.5" aria-hidden="true" />
                                                Roll back
                                              </Button>
                                            )
                                          ) : null}
                                        </div>
                                      </td>
                                    </tr>
                                  ))}
                                </tbody>
                              </table>
                            </div>
                          ) : null}
                          {leftComparedVersion && rightComparedVersion ? (
                            <div className="grid gap-3 rounded-md border border-line bg-white p-4" data-testid={`skill-diff-${skill.name}`}>
                              <div className="flex flex-wrap items-center justify-between gap-2">
                                <div>
                                  <h4 className="text-sm font-semibold text-ink">Version diff</h4>
                                  <p className="text-xs text-ink/55">Comparing v{leftComparedVersion.version} to v{rightComparedVersion.version}</p>
                                </div>
                                <Badge variant="accent">{comparisonDiffEntries.filter((entry) => entry.changed).length} fields changed</Badge>
                              </div>
                              <div className="grid gap-3">
                                {comparisonDiffEntries.map((entry) => (
                                  <section key={entry.key} className="rounded-md border border-line/80 bg-soft/20 p-3">
                                    <div className="mb-2 flex items-center justify-between gap-2">
                                      <h5 className="text-sm font-medium text-ink">{entry.label}</h5>
                                      <Badge variant={entry.changed ? "green" : "gray"}>{entry.changed ? "Changed" : "Same"}</Badge>
                                    </div>
                                    <div className="grid gap-3 md:grid-cols-2">
                                      <DiffValueCard label={`v${leftComparedVersion.version}`} value={entry.before} code={entry.code} />
                                      <DiffValueCard label={`v${rightComparedVersion.version}`} value={entry.after} code={entry.code} />
                                    </div>
                                  </section>
                                ))}
                              </div>
                            </div>
                          ) : null}
                          {versionsQuery.data && versionsQuery.data.length === 0 ? (
                            <p className="text-sm text-ink/60">No version history recorded yet.</p>
                          ) : null}
                        </div>
                      </td>
                    </tr>
                  ) : null}
                  </Fragment>
                ))}
              </tbody>
            </table>
          </div>
        </section>
      ) : null}

      {drawerOpen ? (
        <div className="fixed inset-0 z-40 bg-ink/25" role="presentation" onMouseDown={(event) => event.target === event.currentTarget && resetDrawer()}>
          <aside className="ml-auto grid h-full w-full max-w-xl grid-rows-[auto_1fr] border-l border-line bg-white shadow-xl" role="dialog" aria-modal="true">
            <div className="flex items-center justify-between border-b border-line px-5 py-4">
              <h2 className="inline-flex items-center gap-1.5 text-lg font-semibold text-ink">
                <span>{editingSkill ? "Edit Skill" : "Add Skill"}</span>
                <HelpTooltip label={editingSkill ? "Edit Skill" : "Add Skill"}>Configure an HTTP tool MemoryOps can expose to agents during retrieval workflows.</HelpTooltip>
              </h2>
              <Button type="button" variant="ghost" size="icon" aria-label="Close" onClick={resetDrawer}>
                <X className="h-4 w-4" aria-hidden="true" />
              </Button>
            </div>
            <form className="thin-scrollbar grid content-start gap-4 overflow-y-auto p-5" onSubmit={submitSkill}>
              <Field label="Name" helpText="Machine-safe identifier. Use lowercase letters, numbers, and underscores." error={errors.name}>
                <Input data-testid="skill-form-name" value={draft.name} onChange={(event) => updateDraft("name", event.target.value)} disabled={Boolean(editingSkill)} />
              </Field>
              <Field label="Description" helpText="What this skill does so agents and operators know when to call it." error={errors.description}>
                <Input data-testid="skill-form-description" value={draft.description} onChange={(event) => updateDraft("description", event.target.value)} />
              </Field>
              <Field label="URL" helpText="HTTPS endpoint MemoryOps will call when this skill is invoked. Local, private, and network metadata URLs should be rejected by the backend." error={errors.endpoint_url}>
                <Input data-testid="skill-form-endpoint_url" value={draft.endpoint_url} onChange={(event) => updateDraft("endpoint_url", event.target.value)} />
              </Field>
              <div className="grid gap-4 sm:grid-cols-2">
                <Field label="Method" helpText="HTTP method MemoryOps will use when invoking the skill." error={errors.http_method}>
                  <select
                    data-testid="skill-form-http_method"
                    value={draft.http_method}
                    onChange={(event) => updateDraft("http_method", event.target.value)}
                    className="h-10 rounded-md border border-line bg-white px-3 text-sm text-ink outline-none focus:border-accent focus:ring-2 focus:ring-accent/20"
                  >
                    <option value="GET">GET</option>
                    <option value="POST">POST</option>
                    <option value="PUT">PUT</option>
                  </select>
                </Field>
                <Field label="Visibility" helpText="Private skills stay hidden from MCP retrieval and invocation. Workspace and published skills remain workspace-accessible, with published marking the skill as broadly reusable." error={errors.scope_visibility}>
                  <select
                    data-testid="skill-form-scope_visibility"
                    value={draft.scope_visibility}
                    onChange={(event) => updateDraft("scope_visibility", event.target.value)}
                    className="h-10 rounded-md border border-line bg-white px-3 text-sm text-ink outline-none focus:border-accent focus:ring-2 focus:ring-accent/20"
                  >
                    <option value="private">Private</option>
                    <option value="workspace">Workspace</option>
                    <option value="published">Published</option>
                  </select>
                </Field>
              </div>
              <div className="grid gap-4 sm:grid-cols-2">
                <Field label="Auth header" helpText="Header name MemoryOps should send when authenticating to the skill endpoint." error={errors.auth_header}>
                  <Input data-testid="skill-form-auth_header" value={draft.auth_header} onChange={(event) => updateDraft("auth_header", event.target.value)} placeholder="Authorization" />
                </Field>
                <div className="hidden sm:block" />
              </div>
              <Field label="Auth secret" helpText="Secret value stored encrypted by the backend. It is not re-displayed after save." error={errors.auth_secret}>
                <Input data-testid="skill-form-auth_secret" type="password" value={draft.auth_secret} onChange={(event) => updateDraft("auth_secret", event.target.value)} />
              </Field>
              <Field label="Input schema" helpText="JSON Schema describing what the agent should send to this skill." error={errors.input_schema}>
                <textarea data-testid="skill-form-input_schema" value={draft.input_schema} onChange={(event) => updateDraft("input_schema", event.target.value)} className="min-h-32 rounded-md border border-line bg-white px-3 py-2 font-mono text-sm outline-none focus:border-accent focus:ring-2 focus:ring-accent/20" />
              </Field>
              <Field label="Output schema" helpText="JSON Schema describing what the skill returns." error={errors.output_schema}>
                <textarea data-testid="skill-form-output_schema" value={draft.output_schema} onChange={(event) => updateDraft("output_schema", event.target.value)} className="min-h-32 rounded-md border border-line bg-white px-3 py-2 font-mono text-sm outline-none focus:border-accent focus:ring-2 focus:ring-accent/20" />
              </Field>
              <div className="flex justify-end gap-2 border-t border-line pt-4">
                <Button type="button" variant="secondary" onClick={resetDrawer}>Cancel</Button>
                <Button type="submit" disabled={formPending}>
                  {formPending ? <Loader2 className="h-4 w-4 animate-spin" aria-hidden="true" /> : <Check className="h-4 w-4" aria-hidden="true" />}
                  Save Skill
                </Button>
              </div>
            </form>
          </aside>
        </div>
      ) : null}
    </div>
  );
}

function Field({ label, helpText, error, children }: { label: string; helpText: string; error?: string | undefined; children: React.ReactNode }) {
  return (
    <label className="grid gap-1 text-sm text-ink/70">
      <span className="text-xs font-medium uppercase text-ink/45"><InfoLabel label={label} tooltip={helpText} /></span>
      {children}
      {error ? <span className="text-xs font-medium text-rust">{error}</span> : null}
    </label>
  );
}

function validateDraft(draft: SkillDraft, editing: boolean): { payload?: Partial<CreateSkillPayload>; errors: FormErrors } {
  const errors: FormErrors = {};
  const name = draft.name.trim();
  const description = draft.description.trim();
  const endpointUrl = draft.endpoint_url.trim();
  const authHeader = draft.auth_header.trim();
  const authSecret = draft.auth_secret.trim();
  const inputSchema = parseSchema(draft.input_schema, "input_schema", errors);
  const outputSchema = parseSchema(draft.output_schema, "output_schema", errors);

  if (!editing && !skillNamePattern.test(name)) {
    errors.name = "Use lowercase letters, digits, and underscores.";
  }
  if (description.length === 0 || description.length > 500) {
    errors.description = "Enter 1-500 characters.";
  }
  if (!endpointUrl.startsWith("https://")) {
    errors.endpoint_url = "URL must start with https://.";
  }
  if (authSecret && !authHeader) {
    errors.auth_header = "Auth header is required when a secret is set.";
  }

  if (Object.values(errors).some(Boolean)) {
    return { errors };
  }

  const payload: Partial<CreateSkillPayload> = {
    description,
    endpoint_url: endpointUrl,
    http_method: draft.http_method,
    scope_visibility: draft.scope_visibility,
    input_schema: inputSchema,
    output_schema: outputSchema,
  };
  if (!editing) {
    payload.name = name;
  }
  if (authHeader) {
    payload.auth_header = authHeader;
  }
  if (authSecret) {
    payload.auth_secret = authSecret;
  }

  return { payload, errors };
}

function parseSchema(text: string, field: "input_schema" | "output_schema", errors: FormErrors): unknown {
  try {
    const parsed = JSON.parse(text || "{}");
    if (!parsed || typeof parsed !== "object" || Array.isArray(parsed)) {
      errors[field] = "Schema must be a JSON object.";
    }
    return parsed;
  } catch {
    errors[field] = "Enter valid JSON.";
    return {};
  }
}

function toggleClass(enabled: boolean): string {
  return cn(
    "inline-flex h-6 w-11 items-center rounded-full border p-0.5 transition focus:outline-none focus:ring-2 focus:ring-accent disabled:opacity-60",
    enabled ? "border-green-500 bg-green-500" : "border-line bg-ink/20",
  );
}

function SkillsSkeleton() {
  return (
    <div className="rounded-lg border border-line bg-white p-4">
      {Array.from({ length: 5 }, (_, index) => (
        <div key={index} className={cn("grid gap-4 py-4 md:grid-cols-[10rem_1fr_6rem_1fr_8rem]", index > 0 && "border-t border-line")}>
          <Skeleton className="h-4 w-32" />
          <Skeleton className="h-4 w-full" />
          <Skeleton className="h-6 w-16" />
          <Skeleton className="h-4 w-full" />
          <Skeleton className="h-9 w-24" />
        </div>
      ))}
    </div>
  );
}

function skillsKey(workspaceId: string) {
  return ["workspace", workspaceId, "skills"] as const;
}

function skillVersionsKey(workspaceId: string, name: string) {
  return ["workspace", workspaceId, "skills", name, "versions"] as const;
}

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : "Skills could not be loaded.";
}

function statusColor(status: number): string {
  if (status < 300) return "font-semibold text-green-600";
  if (status < 500) return "font-semibold text-yellow-600";
  return "font-semibold text-rust";
}

function TooltipText({ value, children }: { value: string; children: React.ReactNode }) {
  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <span tabIndex={0} className="inline-block rounded-sm focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent">
          {children}
        </span>
      </TooltipTrigger>
      <TooltipContent>{value}</TooltipContent>
    </Tooltip>
  );
}

function DiffValueCard({ label, value, code }: { label: string; value: string; code: boolean }) {
  return (
    <div className="grid gap-1">
      <span className="text-[11px] font-medium uppercase tracking-wide text-ink/45">{label}</span>
      {code ? (
        <pre className="thin-scrollbar max-h-60 overflow-auto rounded-md bg-ink px-3 py-2 font-mono text-xs text-white/90 whitespace-pre-wrap break-words">{value}</pre>
      ) : (
        <div className="rounded-md border border-line bg-white px-3 py-2 text-xs text-ink/75 whitespace-pre-wrap break-words">{value}</div>
      )}
    </div>
  );
}

type SkillVersionDiffEntry = {
  key: string;
  label: string;
  before: string;
  after: string;
  changed: boolean;
  code: boolean;
};

function buildSkillVersionDiffEntries(left: SkillVersion, right: SkillVersion): SkillVersionDiffEntry[] {
  const entries: Array<Omit<SkillVersionDiffEntry, "changed">> = [
    { key: "description", label: "Description", before: left.description, after: right.description, code: false },
    { key: "endpoint_url", label: "Endpoint URL", before: left.endpoint_url, after: right.endpoint_url, code: false },
    { key: "http_method", label: "HTTP Method", before: left.http_method, after: right.http_method, code: false },
    { key: "input_schema", label: "Input schema", before: formatSkillDiffJson(left.input_schema), after: formatSkillDiffJson(right.input_schema), code: true },
    { key: "output_schema", label: "Output schema", before: formatSkillDiffJson(left.output_schema), after: formatSkillDiffJson(right.output_schema), code: true },
    { key: "auth_header", label: "Auth header", before: formatSkillAuthHeader(left.auth_header), after: formatSkillAuthHeader(right.auth_header), code: false },
    { key: "enabled", label: "Enabled", before: left.enabled ? "Enabled" : "Disabled", after: right.enabled ? "Enabled" : "Disabled", code: false },
    { key: "scope_visibility", label: "Scope visibility", before: formatSkillScopeVisibility(left.scope_visibility), after: formatSkillScopeVisibility(right.scope_visibility), code: false },
  ];

  return entries.map((entry) => ({
    ...entry,
    changed: entry.before !== entry.after,
  }));
}

function formatSkillDiffJson(value: unknown): string {
  return JSON.stringify(value ?? {}, null, 2);
}

function formatSkillAuthHeader(value: string | null): string {
  return value ? `Configured header: ${value}` : "No auth header configured";
}

function formatSkillScopeVisibility(value: SkillVersion["scope_visibility"]): string {
  return value.charAt(0).toUpperCase() + value.slice(1);
}

function scopeBadgeVariant(value: SkillVersion["scope_visibility"]): "gray" | "green" | "teal" {
  if (value === "published") {
    return "teal";
  }
  if (value === "workspace") {
    return "green";
  }
  return "gray";
}
