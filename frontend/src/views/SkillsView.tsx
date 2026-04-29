import { Check, Edit3, FlaskConical, Loader2, Play, Plus, Trash2, X } from "lucide-react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Fragment, useEffect, useMemo, useState, type FormEvent } from "react";

import { createSkill, deleteSkill, listSkills, testSkill, updateSkill, type CreateSkillPayload, type Skill, type SkillTestResponse } from "../api/skills";
import type { JsonValue } from "../api/types";
import { EmptyState } from "../components/EmptyState";
import { InlineError } from "../components/InlineError";
import { Badge } from "../components/ui/badge";
import { Button } from "../components/ui/button";
import { Input } from "../components/ui/input";
import { Skeleton } from "../components/ui/skeleton";
import { previewText } from "../lib/format";
import { cn } from "../lib/utils";
import { useAppStore } from "../store/app-store";

const skillNamePattern = /^[a-z][a-z0-9_]{0,63}$/;
const emptyDraft = {
  name: "",
  description: "",
  endpoint_url: "https://",
  http_method: "POST",
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

  const rows = useMemo(() => skillsQuery.data ?? [], [skillsQuery.data]);
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
    setTestBody(JSON.stringify(skill.input_schema ?? {}, null, 2));
    setTestResult(null);
    setTestError(null);
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
        <Button type="button" data-testid="skill-add-button" onClick={openCreateDrawer} disabled={!hasAuth}>
          <Plus className="h-4 w-4" aria-hidden="true" />
          Add Skill
        </Button>
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
                  <th className="px-4 py-3">Name</th>
                  <th className="px-4 py-3">Description</th>
                  <th className="px-4 py-3">Method</th>
                  <th className="px-4 py-3">Endpoint</th>
                  <th className="px-4 py-3">Enabled</th>
                  <th className="px-4 py-3 text-right">Actions</th>
                </tr>
              </thead>
              <tbody>
                {rows.map((skill) => (
                  <Fragment key={skill.id}>
                  <tr data-testid={`skill-row-${skill.name}`} className="border-b border-line/80 last:border-b-0">
                    <td className="px-4 py-4 align-middle font-mono text-sm text-ink">{skill.name}</td>
                    <td className="max-w-[22rem] px-4 py-4 align-middle text-sm text-ink/70">{previewText(skill.description, 96)}</td>
                    <td className="px-4 py-4 align-middle">
                      <Badge variant="gray">{skill.http_method}</Badge>
                    </td>
                    <td className="max-w-[24rem] truncate px-4 py-4 align-middle font-mono text-xs text-ink/60" title={skill.endpoint_url}>{skill.endpoint_url}</td>
                    <td className="px-4 py-4 align-middle">
                      <button
                        type="button"
                        data-testid={`skill-enabled-${skill.name}`}
                        className={toggleClass(skill.enabled)}
                        onClick={() => toggleEnabled(skill)}
                        disabled={updateMutation.isPending}
                        aria-pressed={skill.enabled}
                      >
                        <span className={cn("h-4 w-4 rounded-full bg-white shadow transition", skill.enabled ? "translate-x-5" : "translate-x-0")} />
                      </button>
                    </td>
                    <td className="relative px-4 py-4 align-middle">
                      <div className="flex justify-end gap-2">
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
                        <Button type="button" variant="ghost" size="icon" data-testid={`skill-edit-${skill.name}`} aria-label={`Edit ${skill.name}`} onClick={() => openEditDrawer(skill)}>
                          <Edit3 className="h-4 w-4" aria-hidden="true" />
                        </Button>
                        <Button type="button" variant="ghost" size="icon" data-testid={`skill-delete-${skill.name}`} aria-label={`Delete ${skill.name}`} onClick={() => setConfirmingDelete(skill.name)}>
                          <Trash2 className="h-4 w-4" aria-hidden="true" />
                        </Button>
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
                              <span className="text-xs font-medium uppercase text-ink/45">Method</span>
                              <p className="mt-0.5 font-mono text-ink">{skill.http_method}</p>
                            </div>
                            <div className="min-w-0 flex-1">
                              <span className="text-xs font-medium uppercase text-ink/45">Endpoint</span>
                              <p className="mt-0.5 truncate font-mono text-xs text-ink/70">{skill.endpoint_url}</p>
                            </div>
                          </div>
                          <label className="grid gap-1">
                            <span className="text-xs font-medium uppercase text-ink/45">Request body (JSON)</span>
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
                              Run
                            </Button>
                            {testResult ? (
                              <span className="text-sm text-ink/60">
                                <span className={statusColor(testResult.status)}>{testResult.status}</span>
                                {" · "}{testResult.latency_ms} ms
                              </span>
                            ) : null}
                          </div>
                          {testResult ? (
                            <pre
                              data-testid={`skill-test-response-${skill.name}`}
                              className="max-h-64 overflow-auto rounded-md bg-ink px-4 py-3 font-mono text-xs text-white/90"
                            >
                              {JSON.stringify(testResult.body, null, 2)}
                            </pre>
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
              <h2 className="text-lg font-semibold text-ink">{editingSkill ? "Edit Skill" : "Add Skill"}</h2>
              <Button type="button" variant="ghost" size="icon" aria-label="Close" onClick={resetDrawer}>
                <X className="h-4 w-4" aria-hidden="true" />
              </Button>
            </div>
            <form className="thin-scrollbar grid content-start gap-4 overflow-y-auto p-5" onSubmit={submitSkill}>
              <Field label="Name" error={errors.name}>
                <Input data-testid="skill-form-name" value={draft.name} onChange={(event) => updateDraft("name", event.target.value)} disabled={Boolean(editingSkill)} />
              </Field>
              <Field label="Description" error={errors.description}>
                <Input data-testid="skill-form-description" value={draft.description} onChange={(event) => updateDraft("description", event.target.value)} />
              </Field>
              <Field label="URL" error={errors.endpoint_url}>
                <Input data-testid="skill-form-endpoint_url" value={draft.endpoint_url} onChange={(event) => updateDraft("endpoint_url", event.target.value)} />
              </Field>
              <div className="grid gap-4 sm:grid-cols-2">
                <Field label="Method" error={errors.http_method}>
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
                <Field label="Auth header" error={errors.auth_header}>
                  <Input data-testid="skill-form-auth_header" value={draft.auth_header} onChange={(event) => updateDraft("auth_header", event.target.value)} placeholder="Authorization" />
                </Field>
              </div>
              <Field label="Auth secret" error={errors.auth_secret}>
                <Input data-testid="skill-form-auth_secret" type="password" value={draft.auth_secret} onChange={(event) => updateDraft("auth_secret", event.target.value)} />
              </Field>
              <Field label="Input schema" error={errors.input_schema}>
                <textarea data-testid="skill-form-input_schema" value={draft.input_schema} onChange={(event) => updateDraft("input_schema", event.target.value)} className="min-h-32 rounded-md border border-line bg-white px-3 py-2 font-mono text-sm outline-none focus:border-accent focus:ring-2 focus:ring-accent/20" />
              </Field>
              <Field label="Output schema" error={errors.output_schema}>
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

function Field({ label, error, children }: { label: string; error?: string | undefined; children: React.ReactNode }) {
  return (
    <label className="grid gap-1 text-sm text-ink/70">
      <span className="text-xs font-medium uppercase text-ink/45">{label}</span>
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

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : "Skills could not be loaded.";
}

function statusColor(status: number): string {
  if (status < 300) return "font-semibold text-green-600";
  if (status < 500) return "font-semibold text-yellow-600";
  return "font-semibold text-rust";
}
