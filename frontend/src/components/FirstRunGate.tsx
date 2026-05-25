import {
  Check,
  ChevronDown,
  Clipboard,
  KeyRound,
  Loader2,
  Sparkles,
  TriangleAlert,
} from "lucide-react";
import { useMutation, useQuery } from "@tanstack/react-query";
import type { FormEvent, ReactNode } from "react";
import { useEffect, useState } from "react";

import { createApiKey, createWorkspace, listWorkspaces } from "../api/workspaces";
import { useAppStore } from "../store/app-store";
import { InlineError } from "./InlineError";
import { Button } from "./ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "./ui/card";
import { Input } from "./ui/input";

type FirstRunGateProps = {
  children: ReactNode;
};

type FirstRunStep = "workspace" | "key";

export function FirstRunGate({ children }: FirstRunGateProps) {
  const workspaceId = useAppStore((state) => state.workspaceId);
  const apiKey = useAppStore((state) => state.apiKey);
  const setWorkspace = useAppStore((state) => state.setWorkspace);
  const setWorkspaceId = useAppStore((state) => state.setWorkspaceId);
  const [workspaceName, setWorkspaceName] = useState("MemoryOps Workspace");
  const [adminToken, setAdminToken] = useState("");
  const [step, setStep] = useState<FirstRunStep>(() =>
    workspaceId.trim().length > 0 ? "key" : "workspace"
  );
  const [plaintextKey, setPlaintextKey] = useState("");
  const [copied, setCopied] = useState(false);
  const [workspaceCopied, setWorkspaceCopied] = useState(false);

  // Fix #2 — controlled inputs replacing raw DOM access
  const [existingKey, setExistingKey] = useState("");
  const [connectWorkspaceId, setConnectWorkspaceId] = useState("");
  const [connectApiKey, setConnectApiKey] = useState("");

  // Fix #1 — collapsible "Already have a workspace?" section
  const [connectOpen, setConnectOpen] = useState(false);

  // Workspace discovery query — fires only when key looks valid
  const workspacesQuery = useQuery({
    queryKey: ["workspaces", connectApiKey],
    queryFn: () => listWorkspaces(connectApiKey),
    enabled: connectApiKey.startsWith("mops_") && connectApiKey.length > 20,
    staleTime: 30_000,
    retry: false,
  });

  // Auto-select when exactly one workspace comes back
  useEffect(() => {
    const first = workspacesQuery.data?.[0];
    if (workspacesQuery.data?.length === 1 && !connectWorkspaceId && first) {
      setConnectWorkspaceId(first.id);
    }
  }, [workspacesQuery.data, connectWorkspaceId]);

  const workspaceMutation = useMutation({
    mutationKey: ["first-run", "workspace"],
    mutationFn: ({ name, token }: { name: string; token: string }) =>
      createWorkspace(name.trim(), token.trim()),
    onSuccess: (workspace) => {
      setWorkspaceId(workspace.id);
      if (workspace.api_key) {
        setPlaintextKey(workspace.api_key);
        setCopied(false);
      }
      setStep("key");
    },
  });

  const keyMutation = useMutation({
    mutationKey: ["first-run", workspaceId, "key"],
    mutationFn: () => createApiKey(workspaceId, "default"),
    onSuccess: (result) => {
      setPlaintextKey(result.plaintext_key);
      setCopied(false);
    },
  });

  const isReady = workspaceId.trim().length > 0 && apiKey.trim().length > 0;
  if (isReady) {
    return <>{children}</>;
  }

  function submitWorkspace(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const name = workspaceName.trim();
    const token = adminToken.trim();
    if (name.length > 0 && token.length > 0) {
      workspaceMutation.mutate({ name, token });
    }
  }

  function copyKey() {
    if (!plaintextKey) {
      return;
    }
    void navigator.clipboard.writeText(plaintextKey).then(() => setCopied(true));
  }

  // Fix #3 — copy workspace ID
  function copyWorkspaceId() {
    if (!workspaceId) {
      return;
    }
    void navigator.clipboard
      .writeText(workspaceId)
      .then(() => {
        setWorkspaceCopied(true);
        setTimeout(() => setWorkspaceCopied(false), 2000);
      });
  }

  function finishSetup() {
    if (workspaceId.trim().length > 0 && plaintextKey.trim().length > 0) {
      setWorkspace(workspaceId, plaintextKey);
    }
  }

  return (
    <div className="grid min-h-screen place-items-center bg-soft px-4 py-8 text-ink">
      <Card className="w-full max-w-lg shadow-xl">
        <CardHeader>
          <div className="mb-3 grid h-11 w-11 place-items-center rounded-lg bg-accent text-white">
            <Sparkles className="h-5 w-5" aria-hidden="true" />
          </div>
          <CardTitle>Set Up MemoryOps</CardTitle>
          <p className="text-sm text-ink/65">
            Create a workspace and generate the first API key for this browser
            session.
          </p>
        </CardHeader>
        <CardContent className="space-y-5">
          {/* Fix #6 — step pills with step number when incomplete */}
          <div className="grid grid-cols-2 gap-2 text-xs font-medium text-ink/60">
            <StepPill
              active={step === "workspace"}
              complete={workspaceId.trim().length > 0}
              label="Workspace"
              step={1}
            />
            <StepPill
              active={step === "key"}
              complete={plaintextKey.trim().length > 0}
              label="API key"
              step={2}
            />
          </div>

          {/* Fix #8 — aria-live region for step content */}
          <div aria-live="polite" aria-atomic="true">
            {step === "workspace" ? (
              <form className="grid gap-4" onSubmit={submitWorkspace}>
                <label className="grid gap-2 text-sm font-medium text-ink/70">
                  Name
                  <Input
                    data-testid="workspace-name-input"
                    value={workspaceName}
                    onChange={(event) => setWorkspaceName(event.target.value)}
                  />
                </label>
                <label className="grid gap-2 text-sm font-medium text-ink/70">
                  Admin token
                  <Input
                    data-testid="workspace-admin-token-input"
                    type="password"
                    autoComplete="current-password"
                    value={adminToken}
                    onChange={(event) => setAdminToken(event.target.value)}
                  />
                </label>
                <Button
                  type="submit"
                  data-testid="create-workspace-button"
                  disabled={
                    workspaceMutation.isPending ||
                    workspaceName.trim().length === 0 ||
                    adminToken.trim().length === 0
                  }
                >
                  {workspaceMutation.isPending ? (
                    <Loader2 className="h-4 w-4 animate-spin" aria-hidden="true" />
                  ) : (
                    <KeyRound className="h-4 w-4" aria-hidden="true" />
                  )}
                  {workspaceMutation.isPending ? "Creating" : "Create Workspace"}
                </Button>
                {workspaceMutation.isError ? (
                  <InlineError message={workspaceMutation.error.message} />
                ) : null}
              </form>
            ) : (
              <div className="grid gap-4">
                {/* Fix #3 — workspace ID display with copy button */}
                <div className="flex min-w-0 items-center gap-2 rounded-md border border-line bg-soft px-3 py-2">
                  <code className="min-w-0 flex-1 truncate font-mono text-xs text-ink/70">
                    {workspaceId}
                  </code>
                  <Button
                    type="button"
                    variant="secondary"
                    size="sm"
                    onClick={copyWorkspaceId}
                    aria-label="Copy workspace ID"
                  >
                    {workspaceCopied ? (
                      <Check className="h-4 w-4" aria-hidden="true" />
                    ) : (
                      <Clipboard className="h-4 w-4" aria-hidden="true" />
                    )}
                    {workspaceCopied ? "Copied" : "Copy"}
                  </Button>
                </div>

                {!plaintextKey ? (
                  <div className="grid gap-4">
                    <Button
                      type="button"
                      data-testid="create-api-key-button"
                      onClick={() => keyMutation.mutate()}
                      disabled={
                        keyMutation.isPending || workspaceId.trim().length === 0
                      }
                    >
                      {keyMutation.isPending ? (
                        <Loader2
                          className="h-4 w-4 animate-spin"
                          aria-hidden="true"
                        />
                      ) : (
                        <KeyRound className="h-4 w-4" aria-hidden="true" />
                      )}
                      {keyMutation.isPending ? "Creating" : "Create API Key"}
                    </Button>

                    {/* Fix #5 — divider uses bg-soft to match card background */}
                    <div className="relative">
                      <div className="absolute inset-0 flex items-center">
                        <span className="w-full border-t border-line" />
                      </div>
                      <div className="relative flex justify-center text-xs uppercase">
                        <span className="bg-soft px-2 text-ink/45">
                          Or use existing
                        </span>
                      </div>
                    </div>

                    {/* Fix #2 — controlled existingKey state */}
                    <div className="flex gap-2">
                      <Input
                        data-testid="existing-key-input"
                        placeholder="Paste API key (mops_...)"
                        value={existingKey}
                        onChange={(e) => setExistingKey(e.target.value)}
                      />
                      <Button
                        type="button"
                        variant="secondary"
                        data-testid="existing-key-submit"
                        onClick={() => {
                          const val = existingKey.trim();
                          if (val) setWorkspace(workspaceId, val);
                        }}
                      >
                        Submit
                      </Button>
                    </div>
                  </div>
                ) : (
                  <div className="grid gap-3">
                    {/* Fix #4 — amber banner with TriangleAlert icon, semibold, role="alert" */}
                    <div
                      role="alert"
                      className="flex items-center gap-2 rounded-md border border-amber-200 bg-amber-50 p-3 text-sm font-semibold text-amber-900"
                    >
                      <TriangleAlert
                        className="h-4 w-4 shrink-0"
                        aria-hidden="true"
                      />
                      Copy this key now — it will not be shown again.
                    </div>
                    <div className="flex min-w-0 items-center gap-2 rounded-md border border-line bg-white p-2">
                      <code className="min-w-0 flex-1 truncate text-xs text-ink/75">
                        {plaintextKey}
                      </code>
                      <Button
                        type="button"
                        variant="secondary"
                        size="sm"
                        data-testid="copy-key-button"
                        onClick={copyKey}
                        aria-label="Copy API key"
                      >
                        {copied ? (
                          <Check className="h-4 w-4" aria-hidden="true" />
                        ) : (
                          <Clipboard className="h-4 w-4" aria-hidden="true" />
                        )}
                        {copied ? "Copied" : "Copy"}
                      </Button>
                    </div>

                    {/* Fix #7 — Continue disabled until copied, success ring when enabled */}
                    <Button
                      type="button"
                      data-testid="finish-setup-button"
                      onClick={finishSetup}
                      disabled={!copied}
                      className={
                        copied
                          ? "ring-2 ring-offset-1 ring-accent/40"
                          : ""
                      }
                    >
                      {copied ? "Continue →" : "Copy key to continue"}
                    </Button>
                  </div>
                )}
                {keyMutation.isError ? (
                  <InlineError message={keyMutation.error.message} />
                ) : null}
              </div>
            )}
          </div>

          {/* Fix #1 — collapsible "Already have a workspace?" section */}
          <div>
            <button
              type="button"
              className="flex w-full items-center justify-between rounded-md px-1 py-2 text-xs font-medium text-ink/50 transition-colors hover:text-ink/75"
              onClick={() => setConnectOpen((prev) => !prev)}
              aria-expanded={connectOpen}
            >
              <span>Already have a workspace?</span>
              <ChevronDown
                className={`h-3.5 w-3.5 transition-transform duration-200 ${connectOpen ? "rotate-180" : ""}`}
                aria-hidden="true"
              />
            </button>

            {connectOpen && (
              <div className="mt-3 grid gap-3">
                {/* API key first — triggers workspace discovery */}
                <label className="grid gap-2 text-sm font-medium text-ink/70">
                  API Key
                  <Input
                    data-testid="api-key-input"
                    type="password"
                    placeholder="mops_..."
                    value={connectApiKey}
                    onChange={(e) => {
                      setConnectApiKey(e.target.value);
                      setConnectWorkspaceId(""); // reset selection on key change
                    }}
                  />
                </label>

                {/* Workspace picker — shown when key is valid and query has settled */}
                {workspacesQuery.isLoading && (
                  <div className="flex items-center gap-2 text-xs text-ink/50">
                    <Loader2 className="h-3.5 w-3.5 animate-spin" aria-hidden="true" />
                    Loading workspaces…
                  </div>
                )}

                {workspacesQuery.isError && (
                  <InlineError message="Could not load workspaces — check your API key." />
                )}

                {workspacesQuery.data && workspacesQuery.data.length === 0 && (
                  <p className="text-xs text-ink/50">No workspaces found for this key.</p>
                )}

                {workspacesQuery.data && workspacesQuery.data.length > 0 && (
                  <div className="grid gap-2">
                    <p className="text-xs font-medium text-ink/70">Select workspace</p>
                    <div className="grid max-h-48 gap-1.5 overflow-y-auto">
                      {workspacesQuery.data.map((ws) => (
                        <button
                          key={ws.id}
                          type="button"
                          onClick={() => setConnectWorkspaceId(ws.id)}
                          className={`flex items-center justify-between rounded-md border px-3 py-2 text-left text-sm transition-colors ${
                            connectWorkspaceId === ws.id
                              ? "border-accent bg-accent/10 text-accent-strong"
                              : "border-line bg-white hover:bg-soft"
                          }`}
                        >
                          <span className="font-medium">{ws.name}</span>
                          <span className="font-mono text-xs text-ink/40">
                            {ws.id.slice(0, 8)}…
                          </span>
                        </button>
                      ))}
                    </div>
                  </div>
                )}

                {/* Manual workspace ID fallback — shown when no workspaces loaded yet */}
                {!workspacesQuery.data && (
                  <label className="grid gap-2 text-sm font-medium text-ink/70">
                    Workspace ID
                    <Input
                      data-testid="workspace-id-input"
                      placeholder="Paste workspace ID"
                      value={connectWorkspaceId}
                      onChange={(e) => setConnectWorkspaceId(e.target.value)}
                    />
                  </label>
                )}

                <Button
                  type="button"
                  data-testid="connect-button"
                  variant="secondary"
                  disabled={!connectWorkspaceId || !connectApiKey}
                  onClick={() => {
                    const wsId = connectWorkspaceId.trim();
                    const key = connectApiKey.trim();
                    if (wsId && key) setWorkspace(wsId, key);
                  }}
                >
                  Connect
                </Button>
              </div>
            )}
          </div>
        </CardContent>
      </Card>
    </div>
  );
}

// Fix #6 — StepPill with step number when incomplete
function StepPill({
  active,
  complete,
  label,
  step,
}: {
  active: boolean;
  complete: boolean;
  label: string;
  step: number;
}) {
  return (
    <div
      className={`flex items-center justify-center gap-2 rounded-md border px-3 py-2 ${
        active
          ? "border-accent bg-accent/10 text-accent-strong"
          : "border-line bg-white"
      }`}
    >
      {complete ? (
        <Check className="h-3.5 w-3.5" aria-hidden="true" />
      ) : (
        <span className="flex h-4 w-4 items-center justify-center rounded-full bg-ink/15 text-[10px] font-semibold">
          {step}
        </span>
      )}
      <span>{label}</span>
    </div>
  );
}
