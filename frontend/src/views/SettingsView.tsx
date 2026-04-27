import { Download, KeyRound, ServerCog, ShieldCheck } from "lucide-react";
import { useMutation } from "@tanstack/react-query";
import type { FormEvent } from "react";
import { useState } from "react";

import type { ProviderDefaults } from "../api/types";
import { createWorkspace, createWorkspaceKey, downloadWorkspaceExport } from "../api/workspace";
import { InlineError } from "../components/InlineError";
import { Badge } from "../components/ui/badge";
import { Button } from "../components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "../components/ui/card";
import { Input } from "../components/ui/input";
import { useAppStore } from "../store/app-store";

const providerDefaults: ProviderDefaults = {
  embedding: {
    provider: "fastembed",
    model: "BAAI/bge-small-en-v1.5",
  },
  llm: {
    provider: "ollama",
    model: "llama3",
    baseUrl: "http://localhost:11434",
  },
};

type BootstrapResult = {
  workspaceId: string;
  apiKey: string;
  prefix: string;
};

export function SettingsView() {
  const workspaceId = useAppStore((state) => state.workspaceId);
  const apiKey = useAppStore((state) => state.apiKey);
  const setWorkspaceId = useAppStore((state) => state.setWorkspaceId);
  const setApiKey = useAppStore((state) => state.setApiKey);
  const [workspaceName, setWorkspaceName] = useState("MemoryOps Workspace");

  const bootstrap = useMutation<BootstrapResult, Error, string>({
    mutationKey: ["workspace", "bootstrap"],
    mutationFn: async (name) => {
      const workspace = await createWorkspace(name.trim());
      const key = await createWorkspaceKey(workspace.workspace_id, "frontend-session");
      return {
        workspaceId: workspace.workspace_id,
        apiKey: key.key,
        prefix: key.prefix,
      };
    },
    onSuccess: (result) => {
      setWorkspaceId(result.workspaceId);
      setApiKey(result.apiKey);
    },
  });

  const exportMutation = useMutation<void, Error>({
    mutationKey: ["workspace", workspaceId, "export"],
    mutationFn: () => downloadWorkspaceExport(workspaceId),
  });

  const hasApiKey = apiKey.trim().length > 0;

  function submitBootstrap(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    bootstrap.mutate(workspaceName);
  }

  return (
    <div className="mx-auto grid max-w-7xl gap-6">
      <header className="flex flex-col gap-4 sm:flex-row sm:items-center sm:justify-between">
        <div>
          <p className="text-sm font-medium text-accent-strong">Workspace</p>
          <h1 className="mt-1 text-2xl font-semibold tracking-normal text-ink">Settings</h1>
        </div>
        <Badge variant={hasApiKey ? "green" : "amber"}>
          <ShieldCheck className="mr-1 h-3 w-3" aria-hidden="true" />
          {hasApiKey ? "API key loaded" : "Setup needed"}
        </Badge>
      </header>

      <section className="grid gap-4 lg:grid-cols-[1fr_1fr]">
        <Card>
          <CardHeader className="flex flex-row items-center justify-between space-y-0">
            <CardTitle>Workspace</CardTitle>
            <KeyRound className="h-4 w-4 text-accent-strong" aria-hidden="true" />
          </CardHeader>
          <CardContent className="space-y-4">
            {!hasApiKey ? (
              <form className="grid gap-3" onSubmit={submitBootstrap}>
                <label className="grid gap-2 text-sm font-medium text-ink/70">
                  Name
                  <Input value={workspaceName} onChange={(event) => setWorkspaceName(event.target.value)} />
                </label>
                <Button type="submit" disabled={bootstrap.isPending || workspaceName.trim().length === 0}>
                  <KeyRound className="h-4 w-4" aria-hidden="true" />
                  {bootstrap.isPending ? "Creating" : "Create Workspace"}
                </Button>
                {bootstrap.isError ? <InlineError message={bootstrap.error.message} /> : null}
              </form>
            ) : (
              <div className="grid gap-3">
                <Field label="Workspace ID" value={workspaceId} />
                <Field label="Key prefix" value={apiKey.slice(0, 8)} />
              </div>
            )}
          </CardContent>
        </Card>

        <Card>
          <CardHeader className="flex flex-row items-center justify-between space-y-0">
            <CardTitle>Export</CardTitle>
            <Download className="h-4 w-4 text-accent-strong" aria-hidden="true" />
          </CardHeader>
          <CardContent className="space-y-4">
            <Button type="button" onClick={() => exportMutation.mutate()} disabled={!hasApiKey || exportMutation.isPending}>
              <Download className="h-4 w-4" aria-hidden="true" />
              {exportMutation.isPending ? "Preparing" : "Download JSONL"}
            </Button>
            {exportMutation.isError ? <InlineError message={exportMutation.error.message} /> : null}
          </CardContent>
        </Card>
      </section>

      <Card>
        <CardHeader className="flex flex-row items-center justify-between space-y-0">
          <CardTitle>Provider config</CardTitle>
          <ServerCog className="h-4 w-4 text-accent-strong" aria-hidden="true" />
        </CardHeader>
        <CardContent className="grid gap-4 sm:grid-cols-2">
          <ProviderBlock title="Embedding" rows={[providerDefaults.embedding.provider, providerDefaults.embedding.model]} />
          <ProviderBlock title="LLM" rows={[providerDefaults.llm.provider, providerDefaults.llm.model, providerDefaults.llm.baseUrl]} />
        </CardContent>
      </Card>
    </div>
  );
}

function Field({ label, value }: { label: string; value: string }) {
  return (
    <div>
      <p className="text-xs font-medium uppercase text-ink/45">{label}</p>
      <p className="mt-1 break-all rounded-md border border-line bg-soft px-3 py-2 font-mono text-sm">{value}</p>
    </div>
  );
}

function ProviderBlock({ title, rows }: { title: string; rows: string[] }) {
  return (
    <div className="rounded-lg border border-line bg-soft p-4">
      <p className="text-sm font-semibold">{title}</p>
      <div className="mt-3 grid gap-2">
        {rows.map((row) => (
          <p key={row} className="break-all font-mono text-xs text-ink/70">
            {row}
          </p>
        ))}
      </div>
    </div>
  );
}
