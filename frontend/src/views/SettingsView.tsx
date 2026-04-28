import { AlertCircle, Download, GitMerge, KeyRound, Loader2, Play, ServerCog, ShieldCheck, SlidersHorizontal, X } from "lucide-react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useEffect, useState } from "react";

import type { ProviderDefaults, PromotionReport, WorkspaceConfig } from "../api/types";
import { exportMemories, getWorkspace, triggerPromotion, updateWorkspaceConfig } from "../api/workspaces";
import { InlineError } from "../components/InlineError";
import { Badge } from "../components/ui/badge";
import { Button } from "../components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "../components/ui/card";
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

export function SettingsView() {
  const workspaceId = useAppStore((state) => state.workspaceId);
  const apiKey = useAppStore((state) => state.apiKey);
  const queryClient = useQueryClient();
  const [exportError, setExportError] = useState<string | null>(null);
  const [configError, setConfigError] = useState<string | null>(null);
  const [promotionError, setPromotionError] = useState<string | null>(null);
  const [promotionResult, setPromotionResult] = useState<PromotionReport | null>(null);
  const [promotionThreshold, setPromotionThreshold] = useState(0.72);
  const [dedupThreshold, setDedupThreshold] = useState(0.92);
  const hasApiKey = apiKey.trim().length > 0;
  const workspaceQuery = useQuery({
    queryKey: ["workspace", workspaceId, "settings"],
    queryFn: () => getWorkspace(workspaceId),
    enabled: hasApiKey && workspaceId.trim().length > 0,
  });
  const exportMutation = useMutation({
    mutationKey: ["workspace", workspaceId, "export"],
    mutationFn: () => exportMemories(workspaceId),
    onSuccess: (blob) => {
      setExportError(null);
      downloadBlob(blob, exportFilename(workspaceId));
    },
    onError: (error: Error) => setExportError(error.message),
  });
  const configMutation = useMutation({
    mutationKey: ["workspace", workspaceId, "config"],
    mutationFn: (patch: WorkspaceConfig) => updateWorkspaceConfig(workspaceId, patch),
    onSuccess: (workspace) => {
      setConfigError(null);
      setPromotionThreshold(workspace.promotion_threshold);
      setDedupThreshold(workspace.dedup_cosine_threshold);
    },
    onError: (error: Error) => setConfigError(error.message),
  });
  const promotionMutation = useMutation({
    mutationKey: ["workspace", workspaceId, "promotion"],
    mutationFn: () => triggerPromotion(workspaceId),
    onSuccess: (report) => {
      setPromotionError(null);
      setPromotionResult(report);
      void queryClient.invalidateQueries({ queryKey: ["workspace", workspaceId, "memory"] });
    },
    onError: (error: Error) => {
      setPromotionResult(null);
      setPromotionError(error.message);
    },
  });

  useEffect(() => {
    if (workspaceQuery.data) {
      setPromotionThreshold(workspaceQuery.data.promotion_threshold);
      setDedupThreshold(workspaceQuery.data.dedup_cosine_threshold);
    }
  }, [workspaceQuery.data]);

  function savePromotionThreshold(value: number) {
    setPromotionThreshold(value);
    configMutation.mutate({ promotion_threshold: value });
  }

  function saveDedupThreshold(value: number) {
    setDedupThreshold(value);
    configMutation.mutate({ dedup_cosine_threshold: value });
  }

  return (
    <div className="mx-auto grid max-w-7xl gap-6">
      {exportError ? <ExportErrorToast message={exportError} onDismiss={() => setExportError(null)} /> : null}
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
            <div className="grid gap-3">
              <Field label="Workspace ID" value={workspaceId} />
              <Field label="Key prefix" value={apiKey.slice(0, 8)} />
            </div>
          </CardContent>
        </Card>

        <Card>
          <CardHeader className="flex flex-row items-center justify-between space-y-0">
            <CardTitle>Export Memories</CardTitle>
            <Download className="h-4 w-4 text-accent-strong" aria-hidden="true" />
          </CardHeader>
          <CardContent className="space-y-4">
            <Button type="button" onClick={() => exportMutation.mutate()} disabled={!hasApiKey || workspaceId.trim().length === 0 || exportMutation.isPending}>
              {exportMutation.isPending ? <Loader2 className="h-4 w-4 animate-spin" aria-hidden="true" /> : <Download className="h-4 w-4" aria-hidden="true" />}
              Export JSONL
            </Button>
          </CardContent>
        </Card>
      </section>

      <Card>
        <CardHeader className="flex flex-row items-center justify-between space-y-0">
          <CardTitle>Promotion</CardTitle>
          <GitMerge className="h-4 w-4 text-accent-strong" aria-hidden="true" />
        </CardHeader>
        <CardContent className="grid gap-5 xl:grid-cols-[1fr_1fr_auto] xl:items-end">
          <label className="grid gap-2 text-sm text-ink/70">
            <span className="flex justify-between text-xs font-medium uppercase text-ink/45">
              <span>Promotion threshold: {promotionThreshold.toFixed(2)}</span>
              {configMutation.isPending ? <Loader2 className="h-3.5 w-3.5 animate-spin" aria-hidden="true" /> : <SlidersHorizontal className="h-3.5 w-3.5" aria-hidden="true" />}
            </span>
            <input
              type="range"
              min="0.5"
              max="1"
              step="0.01"
              value={promotionThreshold}
              onChange={(event) => savePromotionThreshold(Number(event.target.value))}
              className="accent-accent"
            />
          </label>

          <label className="grid gap-2 text-sm text-ink/70">
            <span className="flex justify-between text-xs font-medium uppercase text-ink/45">
              <span>Dedup cosine threshold: {dedupThreshold.toFixed(2)}</span>
              {configMutation.isPending ? <Loader2 className="h-3.5 w-3.5 animate-spin" aria-hidden="true" /> : <SlidersHorizontal className="h-3.5 w-3.5" aria-hidden="true" />}
            </span>
            <input
              type="range"
              min="0.8"
              max="0.99"
              step="0.01"
              value={dedupThreshold}
              onChange={(event) => saveDedupThreshold(Number(event.target.value))}
              className="accent-accent"
            />
          </label>

          <div className="grid gap-2">
            <Button
              type="button"
              onClick={() => promotionMutation.mutate()}
              disabled={!hasApiKey || workspaceId.trim().length === 0 || promotionMutation.isPending}
            >
              {promotionMutation.isPending ? <Loader2 className="h-4 w-4 animate-spin" aria-hidden="true" /> : <Play className="h-4 w-4" aria-hidden="true" />}
              Run Promotion Now
            </Button>
            {promotionResult ? (
              <p className="text-sm text-ink/70">Promoted {promotionResult.units_promoted} semantic memories from {promotionResult.clusters_found} clusters</p>
            ) : null}
            {promotionError ? <InlineError title="Promotion failed" message={promotionError} /> : null}
            {configError ? <InlineError title="Config update failed" message={configError} /> : null}
          </div>
        </CardContent>
      </Card>

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

function ExportErrorToast({ message, onDismiss }: { message: string; onDismiss: () => void }) {
  return (
    <div role="alert" className="fixed right-4 top-4 z-50 flex max-w-sm items-start gap-3 rounded-lg border border-orange-200 bg-orange-50 p-4 text-orange-900 shadow-lg">
      <AlertCircle className="mt-0.5 h-4 w-4 shrink-0" aria-hidden="true" />
      <div className="min-w-0 flex-1">
        <p className="text-sm font-semibold">Export failed</p>
        <p className="mt-1 break-words text-sm text-orange-900/80">{message}</p>
      </div>
      <Button type="button" variant="ghost" size="icon" className="h-7 w-7 shrink-0 text-orange-900" onClick={onDismiss} aria-label="Dismiss export error">
        <X className="h-4 w-4" aria-hidden="true" />
      </Button>
    </div>
  );
}

function downloadBlob(blob: Blob, filename: string) {
  const url = URL.createObjectURL(blob);
  const anchor = document.createElement("a");
  anchor.href = url;
  anchor.download = filename;
  document.body.append(anchor);
  anchor.click();
  anchor.remove();
  URL.revokeObjectURL(url);
}

function exportFilename(workspaceId: string): string {
  return `memoryops-export-${workspaceId}-${new Date().toISOString().slice(0, 10)}.jsonl`;
}
