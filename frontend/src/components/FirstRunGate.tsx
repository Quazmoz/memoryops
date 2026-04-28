import { Check, Clipboard, KeyRound, Loader2, Sparkles } from "lucide-react";
import { useMutation } from "@tanstack/react-query";
import type { FormEvent, ReactNode } from "react";
import { useState } from "react";

import { createApiKey, createWorkspace } from "../api/workspaces";
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
  const [step, setStep] = useState<FirstRunStep>(() => (workspaceId.trim().length > 0 ? "key" : "workspace"));
  const [plaintextKey, setPlaintextKey] = useState("");
  const [copied, setCopied] = useState(false);

  const workspaceMutation = useMutation({
    mutationKey: ["first-run", "workspace"],
    mutationFn: (name: string) => createWorkspace(name.trim()),
    onSuccess: (workspace) => {
      setWorkspaceId(workspace.id);
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
    if (name.length > 0) {
      workspaceMutation.mutate(name);
    }
  }

  function copyKey() {
    if (!plaintextKey) {
      return;
    }
    void navigator.clipboard.writeText(plaintextKey).then(() => setCopied(true));
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
          <p className="text-sm text-ink/65">Create a workspace and generate the first API key for this browser session.</p>
        </CardHeader>
        <CardContent className="space-y-5">
          <div className="grid grid-cols-2 gap-2 text-xs font-medium text-ink/60">
            <StepPill active={step === "workspace"} complete={workspaceId.trim().length > 0} label="Workspace" />
            <StepPill active={step === "key"} complete={plaintextKey.trim().length > 0} label="API key" />
          </div>

          {step === "workspace" ? (
            <form className="grid gap-4" onSubmit={submitWorkspace}>
              <label className="grid gap-2 text-sm font-medium text-ink/70">
                Name
                <Input data-testid="workspace-name-input" value={workspaceName} onChange={(event) => setWorkspaceName(event.target.value)} />
              </label>
              <Button type="submit" data-testid="create-workspace-button" disabled={workspaceMutation.isPending || workspaceName.trim().length === 0}>
                {workspaceMutation.isPending ? <Loader2 className="h-4 w-4 animate-spin" aria-hidden="true" /> : <KeyRound className="h-4 w-4" aria-hidden="true" />}
                {workspaceMutation.isPending ? "Creating" : "Create Workspace"}
              </Button>
              {workspaceMutation.isError ? <InlineError message={workspaceMutation.error.message} /> : null}
            </form>
          ) : (
            <div className="grid gap-4">
              <div className="rounded-md border border-line bg-soft px-3 py-2 font-mono text-xs text-ink/70">{workspaceId}</div>
              {!plaintextKey ? (
                <div className="grid gap-4">
                  <Button type="button" data-testid="create-api-key-button" onClick={() => keyMutation.mutate()} disabled={keyMutation.isPending || workspaceId.trim().length === 0}>
                    {keyMutation.isPending ? <Loader2 className="h-4 w-4 animate-spin" aria-hidden="true" /> : <KeyRound className="h-4 w-4" aria-hidden="true" />}
                    {keyMutation.isPending ? "Creating" : "Create API Key"}
                  </Button>
                  <div className="relative">
                    <div className="absolute inset-0 flex items-center"><span className="w-full border-t border-line" /></div>
                    <div className="relative flex justify-center text-xs uppercase"><span className="bg-soft px-2 text-ink/45">Or use existing</span></div>
                  </div>
                  <div className="flex gap-2">
                    <Input data-testid="existing-key-input" id="existing-key-input" placeholder="Paste API key (mops_...)" />
                    <Button type="button" variant="secondary" data-testid="existing-key-submit" onClick={() => {
                      const val = (document.getElementById("existing-key-input") as HTMLInputElement)?.value.trim();
                      if (val) setWorkspace(workspaceId, val);
                    }}>Submit</Button>
                  </div>
                </div>
              ) : (
                <div className="grid gap-3">
                  <div className="rounded-md border border-amber-200 bg-amber-50 p-3 text-sm font-medium text-amber-900">
                    Copy this key now — it will not be shown again.
                  </div>
                  <div className="flex min-w-0 items-center gap-2 rounded-md border border-line bg-white p-2">
                    <code className="min-w-0 flex-1 truncate text-xs text-ink/75">{plaintextKey}</code>
                    <Button type="button" variant="secondary" size="sm" data-testid="copy-key-button" onClick={copyKey} aria-label="Copy API key">
                      {copied ? <Check className="h-4 w-4" aria-hidden="true" /> : <Clipboard className="h-4 w-4" aria-hidden="true" />}
                      {copied ? "Copied" : "Copy"}
                    </Button>
                  </div>
                  <Button type="button" data-testid="finish-setup-button" onClick={finishSetup}>Continue</Button>
                </div>
              )}
              {keyMutation.isError ? <InlineError message={keyMutation.error.message} /> : null}
            </div>
          )}

          <div className="relative">
            <div className="absolute inset-0 flex items-center"><span className="w-full border-t border-line" /></div>
            <div className="relative flex justify-center text-xs uppercase"><span className="bg-panel px-2 text-ink/45">Or connect existing</span></div>
          </div>
          <div className="grid gap-3">
            <label className="grid gap-2 text-sm font-medium text-ink/70">
              Workspace ID
              <Input data-testid="workspace-id-input" placeholder="Paste workspace ID" />
            </label>
            <label className="grid gap-2 text-sm font-medium text-ink/70">
              API Key
              <Input data-testid="api-key-input" type="password" placeholder="mops_..." />
            </label>
            <Button type="button" data-testid="connect-button" variant="secondary" onClick={() => {
              const wsId = (document.querySelector('[data-testid="workspace-id-input"]') as HTMLInputElement)?.value.trim();
              const key = (document.querySelector('[data-testid="api-key-input"]') as HTMLInputElement)?.value.trim();
              if (wsId && key) setWorkspace(wsId, key);
            }}>Connect</Button>
          </div>        </CardContent>
      </Card>
    </div>
  );
}

function StepPill({ active, complete, label }: { active: boolean; complete: boolean; label: string }) {
  return (
    <div className={`flex items-center justify-center gap-2 rounded-md border px-3 py-2 ${active ? "border-accent bg-accent/10 text-accent-strong" : "border-line bg-white"}`}>
      {complete ? <Check className="h-3.5 w-3.5" aria-hidden="true" /> : null}
      <span>{label}</span>
    </div>
  );
}
