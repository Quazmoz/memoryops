import { Activity, CheckCircle2, Download, GitMerge, KeyRound, Loader2, Play, RefreshCw, Save, ServerCog, ShieldAlert, ShieldCheck, SlidersHorizontal, Upload, XCircle } from "lucide-react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useEffect, useRef, useState } from "react";

import type { ImportMemoriesResponse, PromotionReport, WorkspaceConfig } from "../api/types";
import { getSystemHealth, type HealthCheck } from "../api/health";
import { exportMemories, getWorkspace, importMemories, triggerPromotion, triggerReindex, updateWorkspaceConfig } from "../api/workspaces";
import { InlineError } from "../components/InlineError";
import { Badge } from "../components/ui/badge";
import { Button } from "../components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "../components/ui/card";
import { Input } from "../components/ui/input";
import { cn } from "../lib/utils";
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
  const [subAgentPools, setSubAgentPools] = useState("");
  const [contradictionMode, setContradictionMode] = useState<"quarantine" | "auto_resolve">("quarantine");
  const [reindexResult, setReindexResult] = useState<{ enqueued: number; next_cursor: string | null } | null>(null);
  const [reindexError, setReindexError] = useState<string | null>(null);
  const [confirmReindex, setConfirmReindex] = useState(false);
  const embeddingModelTimeoutRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const llmModelTimeoutRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const importFileInputRef = useRef<HTMLInputElement | null>(null);
  const hasApiKey = apiKey.trim().length > 0;
  const canAct = hasApiKey && workspaceId.trim().length > 0;

  const healthQuery = useQuery({
    queryKey: ["health", "system"],
    queryFn: () => getSystemHealth(),
    enabled: canAct,
    refetchInterval: 30_000,
  });

  const reindexMutation = useMutation({
    mutationKey: ["workspace", workspaceId, "reindex"],
    mutationFn: () => triggerReindex(workspaceId, true),
    onSuccess: (result) => {
      setReindexError(null);
      setReindexResult(result);
      setConfirmReindex(false);
    },
    onError: (error: Error) => {
      setReindexError(error.message);
      setConfirmReindex(false);
    },
  });

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
      setSubAgentPools((workspace.sub_agent_pools ?? []).join(", "));
      const mode = workspace.contradiction_mode;
      if (mode === "quarantine" || mode === "auto_resolve") {
        setContradictionMode(mode);
      }
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
      setSubAgentPools((workspaceQuery.data.sub_agent_pools ?? []).join(", "));
      const mode = workspaceQuery.data.contradiction_mode;
      if (mode === "quarantine" || mode === "auto_resolve") {
        setContradictionMode(mode);
      }
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

  function saveSubAgentPools() {
    configMutation.mutate({ sub_agent_pools: commaSeparatedValues(subAgentPools) });
  }

  function toggleContradictionMode() {
    const next = contradictionMode === "quarantine" ? "auto_resolve" : "quarantine";
    setContradictionMode(next);
    configMutation.mutate({ contradiction_mode: next });
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

          <div className="grid gap-2">
            <label className="text-xs font-medium uppercase text-ink/45" htmlFor="sub-agent-pools">
              Sub-Agent Pools
            </label>
            <div className="flex flex-col gap-2 sm:flex-row">
              <Input
                id="sub-agent-pools"
                data-testid="sub-agent-pools-input"
                value={subAgentPools}
                onChange={(event) => setSubAgentPools(event.target.value)}
                placeholder="agent-a, agent-b"
              />
              <Button type="button" variant="secondary" data-testid="sub-agent-pools-save" onClick={saveSubAgentPools} disabled={configMutation.isPending}>
                {configMutation.isPending ? <Loader2 className="h-4 w-4 animate-spin" aria-hidden="true" /> : <Save className="h-4 w-4" aria-hidden="true" />}
                Save
              </Button>
            </div>
          </div>
        </CardContent>
      </Card>

      {/* Contradiction Detection ─────────────────────────────────────── */}
      <Card>
        <CardHeader className="flex flex-row items-center justify-between space-y-0">
          <CardTitle>Contradiction Detection</CardTitle>
          <ShieldAlert className="h-4 w-4 text-accent-strong" aria-hidden="true" />
        </CardHeader>
        <CardContent className="grid gap-4">
          <div className="flex items-start justify-between gap-4">
            <div>
              <p className="text-sm font-medium text-ink">Contradiction Mode</p>
              <p className="mt-1 text-xs text-ink/60">
                {contradictionMode === "quarantine"
                  ? "New contradictions are flagged for manual review"
                  : "When a contradiction is detected, the newer memory wins automatically"}
              </p>
            </div>
            <button
              type="button"
              role="switch"
              aria-checked={contradictionMode === "auto_resolve"}
              data-testid="contradiction-mode-toggle"
              onClick={toggleContradictionMode}
              disabled={configMutation.isPending}
              className={cn(
                "relative inline-flex h-6 w-11 shrink-0 cursor-pointer rounded-full border-2 border-transparent transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent disabled:cursor-not-allowed disabled:opacity-50",
                contradictionMode === "auto_resolve" ? "bg-accent" : "bg-ink/20",
              )}
            >
              <span
                className={cn(
                  "pointer-events-none inline-block h-5 w-5 rounded-full bg-white shadow-lg ring-0 transition-transform",
                  contradictionMode === "auto_resolve" ? "translate-x-5" : "translate-x-0",
                )}
              />
            </button>
          </div>
          <div className="flex items-center gap-2 text-xs text-ink/55">
            <span className={contradictionMode === "quarantine" ? "font-semibold text-accent-strong" : ""}>Quarantine for review</span>
            <span>/</span>
            <span className={contradictionMode === "auto_resolve" ? "font-semibold text-accent-strong" : ""}>Auto-resolve (newer wins)</span>
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

      {/* Re-Index ──────────────────────────────────────────────────────── */}
      <Card>
        <CardHeader className="flex flex-row items-center justify-between space-y-0">
          <CardTitle>Embedding Re-Index</CardTitle>
          <RefreshCw className="h-4 w-4 text-accent-strong" aria-hidden="true" />
        </CardHeader>
        <CardContent className="grid gap-4">
          <p className="text-sm text-ink/70">
            Clears all embeddings and re-generates them with the current provider. Required after switching embedding providers.
          </p>
          {reindexError ? <InlineError title="Re-index failed" message={reindexError} /> : null}
          {reindexResult ? (
            <p className="text-sm text-ink/70">
              Enqueued {reindexResult.enqueued} memories for re-indexing.
              {reindexResult.next_cursor ? " More batches pending — re-index will continue automatically." : ""}
            </p>
          ) : null}
          {confirmReindex ? (
            <div className="flex flex-wrap gap-2 rounded-lg border border-amber-200 bg-amber-50 p-3">
              <p className="w-full text-sm font-medium text-amber-900">
                This will re-embed all memories. Continue?
              </p>
              <Button
                type="button"
                size="sm"
                onClick={() => reindexMutation.mutate()}
                disabled={reindexMutation.isPending}
              >
                {reindexMutation.isPending ? <Loader2 className="h-3.5 w-3.5 animate-spin" aria-hidden="true" /> : null}
                Yes, re-index
              </Button>
              <Button type="button" variant="secondary" size="sm" onClick={() => setConfirmReindex(false)}>
                Cancel
              </Button>
            </div>
          ) : (
            <Button
              type="button"
              variant="secondary"
              data-testid="reindex-button"
              disabled={!canAct || reindexMutation.isPending}
              onClick={() => setConfirmReindex(true)}
            >
              <RefreshCw className="h-4 w-4" aria-hidden="true" />
              Re-Index Workspace
            </Button>
          )}
        </CardContent>
      </Card>

      {/* System Health ─────────────────────────────────────────────────── */}
      <Card>
        <CardHeader className="flex flex-row items-center justify-between space-y-0">
          <CardTitle>System Health</CardTitle>
          <div className="flex items-center gap-2">
            {healthQuery.isFetching ? <Loader2 className="h-4 w-4 animate-spin text-accent-strong" aria-hidden="true" /> : <Activity className="h-4 w-4 text-accent-strong" aria-hidden="true" />}
          </div>
        </CardHeader>
        <CardContent className="grid gap-4">
          {healthQuery.isError ? <InlineError message="Could not fetch system health." /> : null}
          {healthQuery.data ? (
            <>
              <div className="flex flex-wrap gap-2">
                {healthQuery.data.checks.map((check) => (
                  <HealthCheckCard key={check.name} check={check} />
                ))}
              </div>
              <div className="flex items-center gap-3">
                <span className={cn(
                  "text-xs font-semibold uppercase",
                  healthQuery.data.status === "healthy" ? "text-green-700" : healthQuery.data.status === "degraded" ? "text-amber-700" : "text-red-700",
                )}>
                  {healthQuery.data.status}
                </span>
                <Button
                  type="button"
                  variant="secondary"
                  size="sm"
                  onClick={() => void healthQuery.refetch()}
                  disabled={healthQuery.isFetching}
                >
                  Check Now
                </Button>
              </div>
            </>
          ) : !healthQuery.isFetching ? (
            <p className="text-sm text-ink/55">Connect to see system health.</p>
          ) : null}
        </CardContent>
      </Card>
    </div>
  );
}

function HealthCheckCard({ check }: { check: HealthCheck }) {
  const statusIcon = check.status === "ok"
    ? <CheckCircle2 className="h-3.5 w-3.5 text-green-600" />
    : check.status === "warn"
    ? <ShieldAlert className="h-3.5 w-3.5 text-amber-600" />
    : <XCircle className="h-3.5 w-3.5 text-red-600" />;

  return (
    <div className={cn(
      "flex min-w-[120px] flex-col gap-1 rounded-lg border p-3",
      check.status === "ok" ? "border-green-200 bg-green-50" : check.status === "warn" ? "border-amber-200 bg-amber-50" : "border-red-200 bg-red-50",
    )}>
      <div className="flex items-center gap-1.5">
        {statusIcon}
        <span className="text-xs font-semibold capitalize text-ink">{check.name}</span>
      </div>
      {check.latency_ms != null ? <span className="text-xs text-ink/55">{check.latency_ms}ms</span> : null}
      {check.message ? <span className="text-xs text-ink/55">{check.message}</span> : null}
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

function commaSeparatedValues(value: string): string[] {
  const values: string[] = [];
  value.split(",").forEach((item) => {
    const trimmed = item.trim();
    if (trimmed.length > 0 && !values.includes(trimmed)) {
      values.push(trimmed);
    }
  });
  return values;
}
