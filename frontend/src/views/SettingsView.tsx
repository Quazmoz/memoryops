import { Download, GitMerge, KeyRound, Loader2, Play, ServerCog, ShieldCheck, SlidersHorizontal, Upload } from "lucide-react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useEffect, useRef, useState } from "react";

import type { ImportMemoriesResponse, PromotionReport, WorkspaceConfig } from "../api/types";
import { exportMemories, getWorkspace, importMemories, triggerPromotion, updateWorkspaceConfig } from "../api/workspaces";
import { InlineError } from "../components/InlineError";
import { Badge } from "../components/ui/badge";
import { Button } from "../components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "../components/ui/card";
import { Input } from "../components/ui/input";
import { useAppStore } from "../store/app-store";

export function SettingsView() {
  const workspaceId = useAppStore((state) => state.workspaceId);
  const apiKey = useAppStore((state) => state.apiKey);
  const queryClient = useQueryClient();
  const [exportError, setExportError] = useState<string | null>(null);
  const [importError, setImportError] = useState<string | null>(null);
  const [importResult, setImportResult] = useState<ImportMemoriesResponse | null>(null);
  const [configError, setConfigError] = useState<string | null>(null);
  const [promotionError, setPromotionError] = useState<string | null>(null);
  const [promotionResult, setPromotionResult] = useState<PromotionReport | null>(null);
  const [promotionThreshold, setPromotionThreshold] = useState(0.72);
  const [dedupThreshold, setDedupThreshold] = useState(0.92);
  const [decayHalfLife, setDecayHalfLife] = useState(30);
  const [pruningThreshold, setPruningThreshold] = useState(0.10);
  const [embeddingProvider, setEmbeddingProvider] = useState("fastembed");
  const [embeddingModel, setEmbeddingModel] = useState("BAAI/bge-small-en-v1.5");
  const [llmProvider, setLlmProvider] = useState("ollama");
  const [llmModel, setLlmModel] = useState("llama3");
  const embeddingModelTimeoutRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const llmModelTimeoutRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const importFileInputRef = useRef<HTMLInputElement | null>(null);
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
  const importMutation = useMutation({
    mutationKey: ["workspace", workspaceId, "import"],
    mutationFn: (file: File) => importMemories(workspaceId, file),
    onSuccess: (result) => {
      setImportError(null);
      setImportResult(result);
      void queryClient.invalidateQueries({ queryKey: ["workspace", workspaceId, "memory"] });
      void queryClient.invalidateQueries({ queryKey: ["tags", workspaceId] });
    },
    onError: (error: Error) => {
      setImportResult(null);
      setImportError(error.message);
    },
  });
  const configMutation = useMutation({
    mutationKey: ["workspace", workspaceId, "config"],
    mutationFn: (patch: Partial<WorkspaceConfig>) => updateWorkspaceConfig(workspaceId, patch),
    onSuccess: (workspace) => {
      setConfigError(null);
      setPromotionThreshold(workspace.promotion_threshold);
      setDedupThreshold(workspace.dedup_cosine_threshold);
      setDecayHalfLife(workspace.decay_half_life_days ?? 30);
      setPruningThreshold(workspace.pruning_threshold ?? 0.10);
      setEmbeddingProvider(workspace.embedding_provider ?? "fastembed");
      setEmbeddingModel(workspace.embedding_model ?? "BAAI/bge-small-en-v1.5");
      setLlmProvider(workspace.llm_provider ?? "ollama");
      setLlmModel(workspace.llm_model ?? "llama3");
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
      setDecayHalfLife(workspaceQuery.data.decay_half_life_days ?? 30);
      setPruningThreshold(workspaceQuery.data.pruning_threshold ?? 0.10);
      setEmbeddingProvider(workspaceQuery.data.embedding_provider ?? "fastembed");
      setEmbeddingModel(workspaceQuery.data.embedding_model ?? "BAAI/bge-small-en-v1.5");
      setLlmProvider(workspaceQuery.data.llm_provider ?? "ollama");
      setLlmModel(workspaceQuery.data.llm_model ?? "llama3");
    }
  }, [workspaceQuery.data]);

  useEffect(() => {
    return () => {
      if (embeddingModelTimeoutRef.current) {
        clearTimeout(embeddingModelTimeoutRef.current);
      }
      if (llmModelTimeoutRef.current) {
        clearTimeout(llmModelTimeoutRef.current);
      }
    };
  }, []);

  function savePromotionThreshold(value: number) {
    setPromotionThreshold(value);
    configMutation.mutate({ promotion_threshold: value });
  }

  function saveDedupThreshold(value: number) {
    setDedupThreshold(value);
    configMutation.mutate({ dedup_cosine_threshold: value });
  }

  function saveDecayHalfLife(value: number) {
    setDecayHalfLife(value);
    configMutation.mutate({ decay_half_life_days: value });
  }

  function savePruningThreshold(value: number) {
    setPruningThreshold(value);
    configMutation.mutate({ pruning_threshold: value });
  }

  function saveEmbeddingProvider(value: string) {
    setEmbeddingProvider(value);
    configMutation.mutate({ embedding_provider: value });
  }

  function saveLlmProvider(value: string) {
    setLlmProvider(value);
    configMutation.mutate({ llm_provider: value });
  }

  function saveEmbeddingModel(value: string) {
    setEmbeddingModel(value);
    if (embeddingModelTimeoutRef.current) {
      clearTimeout(embeddingModelTimeoutRef.current);
    }
    embeddingModelTimeoutRef.current = setTimeout(() => {
      configMutation.mutate({ embedding_model: value });
    }, 600);
  }

  function saveLlmModel(value: string) {
    setLlmModel(value);
    if (llmModelTimeoutRef.current) {
      clearTimeout(llmModelTimeoutRef.current);
    }
    llmModelTimeoutRef.current = setTimeout(() => {
      configMutation.mutate({ llm_model: value });
    }, 600);
  }

  function chooseImportFile() {
    importFileInputRef.current?.click();
  }

  function handleImportFile(file: File | undefined) {
    if (!file) {
      return;
    }
    importMutation.mutate(file);
  }

  return (
    <div className="mx-auto grid max-w-7xl gap-6">
      {exportError ? <InlineError title="Export failed" message={exportError} /> : null}
      {importError ? <InlineError title="Import failed" message={importError} /> : null}
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
            <CardTitle>Backup</CardTitle>
            <Download className="h-4 w-4 text-accent-strong" aria-hidden="true" />
          </CardHeader>
          <CardContent className="space-y-3">
            <div className="flex flex-wrap gap-2">
              <Button
                type="button"
                data-testid="export-jsonl-button"
                onClick={() => exportMutation.mutate()}
                disabled={!hasApiKey || workspaceId.trim().length === 0 || exportMutation.isPending}
              >
                {exportMutation.isPending ? <Loader2 className="h-4 w-4 animate-spin" aria-hidden="true" /> : <Download className="h-4 w-4" aria-hidden="true" />}
                Export JSONL
              </Button>
              <input
                ref={importFileInputRef}
                data-testid="import-jsonl-input"
                type="file"
                accept=".jsonl,application/x-ndjson"
                className="hidden"
                onChange={(event) => {
                  handleImportFile(event.currentTarget.files?.[0]);
                  event.currentTarget.value = "";
                }}
              />
              <Button
                type="button"
                variant="secondary"
                data-testid="import-jsonl-button"
                onClick={chooseImportFile}
                disabled={!hasApiKey || workspaceId.trim().length === 0 || importMutation.isPending}
              >
                {importMutation.isPending ? <Loader2 className="h-4 w-4 animate-spin" aria-hidden="true" /> : <Upload className="h-4 w-4" aria-hidden="true" />}
                Import JSONL
              </Button>
            </div>
            {importResult ? (
              <p className="text-sm text-ink/70">
                Imported {importResult.imported}; skipped {importResult.skipped}; errors {importResult.errors}
              </p>
            ) : null}
          </CardContent>
        </Card>
      </section>

      <Card>
        <CardHeader className="flex flex-row items-center justify-between space-y-0">
          <CardTitle>Promotion</CardTitle>
          <GitMerge className="h-4 w-4 text-accent-strong" aria-hidden="true" />
        </CardHeader>
        <CardContent className="grid gap-5">
          <div className="grid gap-5 xl:grid-cols-2">
            <label className="grid gap-2 text-sm text-ink/70">
              <span className="flex justify-between text-xs font-medium uppercase text-ink/45">
                <span>Promotion threshold: {promotionThreshold.toFixed(2)}</span>
                {configMutation.isPending ? <Loader2 className="h-3.5 w-3.5 animate-spin" aria-hidden="true" /> : <SlidersHorizontal className="h-3.5 w-3.5" aria-hidden="true" />}
              </span>
              <input
                data-testid="promotion-threshold-slider"
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
                data-testid="dedup-threshold-slider"
                type="range"
                min="0.8"
                max="0.99"
                step="0.01"
                value={dedupThreshold}
                onChange={(event) => saveDedupThreshold(Number(event.target.value))}
                className="accent-accent"
              />
            </label>

            <label className="grid gap-2 text-sm text-ink/70">
              <span className="flex justify-between text-xs font-medium uppercase text-ink/45">
                <span>Decay half-life: {decayHalfLife}d</span>
                {configMutation.isPending ? <Loader2 className="h-3.5 w-3.5 animate-spin" aria-hidden="true" /> : <SlidersHorizontal className="h-3.5 w-3.5" aria-hidden="true" />}
              </span>
              <input
                data-testid="decay-half-life-slider"
                type="range"
                min="1"
                max="365"
                step="1"
                value={decayHalfLife}
                onChange={(event) => saveDecayHalfLife(Number(event.target.value))}
                className="accent-accent"
              />
            </label>

            <label className="grid gap-2 text-sm text-ink/70">
              <span className="flex justify-between text-xs font-medium uppercase text-ink/45">
                <span>Prune below: {pruningThreshold.toFixed(2)}</span>
                {configMutation.isPending ? <Loader2 className="h-3.5 w-3.5 animate-spin" aria-hidden="true" /> : <SlidersHorizontal className="h-3.5 w-3.5" aria-hidden="true" />}
              </span>
              <input
                data-testid="pruning-threshold-slider"
                type="range"
                min="0.01"
                max="0.50"
                step="0.01"
                value={pruningThreshold}
                onChange={(event) => savePruningThreshold(Number(event.target.value))}
                className="accent-accent"
              />
            </label>
          </div>

          <div className="grid gap-2">
            <Button
              type="button"
              data-testid="run-promotion-button"
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
          {configMutation.isPending ? <Loader2 className="h-4 w-4 animate-spin text-accent-strong" aria-hidden="true" /> : <ServerCog className="h-4 w-4 text-accent-strong" aria-hidden="true" />}
        </CardHeader>
        <CardContent className="grid gap-4 sm:grid-cols-2">
          <div className="grid gap-4">
            <div className="grid gap-2">
              <label className="text-xs font-medium uppercase text-ink/45" htmlFor="embedding-provider">
                Embedding provider
              </label>
              <select
                id="embedding-provider"
                data-testid="embedding-provider-select"
                value={embeddingProvider}
                onChange={(event) => saveEmbeddingProvider(event.target.value)}
                className="h-10 w-full rounded-md border border-line bg-white px-3 py-2 text-sm text-ink outline-none transition focus:border-accent focus:ring-2 focus:ring-accent/20"
              >
                <option value="fastembed">fastembed</option>
                <option value="openai">openai</option>
              </select>
            </div>
            <div className="grid gap-2">
              <label className="text-xs font-medium uppercase text-ink/45" htmlFor="embedding-model">
                Embedding model
              </label>
              <Input id="embedding-model" data-testid="embedding-model-input" value={embeddingModel} onChange={(event) => saveEmbeddingModel(event.target.value)} />
            </div>
          </div>

          <div className="grid gap-4">
            <div className="grid gap-2">
              <label className="text-xs font-medium uppercase text-ink/45" htmlFor="llm-provider">
                LLM provider
              </label>
              <select
                id="llm-provider"
                data-testid="llm-provider-select"
                value={llmProvider}
                onChange={(event) => saveLlmProvider(event.target.value)}
                className="h-10 w-full rounded-md border border-line bg-white px-3 py-2 text-sm text-ink outline-none transition focus:border-accent focus:ring-2 focus:ring-accent/20"
              >
                <option value="ollama">ollama</option>
                <option value="openai">openai</option>
                <option value="anthropic">anthropic</option>
              </select>
            </div>
            <div className="grid gap-2">
              <label className="text-xs font-medium uppercase text-ink/45" htmlFor="llm-model">
                LLM model
              </label>
              <Input id="llm-model" data-testid="llm-model-input" value={llmModel} onChange={(event) => saveLlmModel(event.target.value)} />
            </div>
          </div>
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

function exportFilename(workspaceId: string): string {
  return `memoryops-export-${workspaceId}-${new Date().toISOString().slice(0, 10)}.jsonl`;
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
