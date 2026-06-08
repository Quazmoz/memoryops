import { Activity, AlertTriangle, CheckCircle2, Clipboard, Download, GitMerge, KeyRound, Loader2, Play, RefreshCw, Save, ServerCog, Shield, ShieldAlert, ShieldCheck, SlidersHorizontal, Upload, XCircle } from "lucide-react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useEffect, useRef, useState } from "react";

import type { ApiKeySummary, ForgetUserDataResponse, ImportMemoriesResponse, PromotionReport, WorkspaceConfig } from "../api/types";
import type { HealthCheck } from "../api/health";
import { createApiKey, exportMemories, forgetUserData, getWorkspace, importMemories, listApiKeys, revokeApiKey, triggerPromotion, triggerReindex, updateWorkspaceConfig } from "../api/workspaces";
import { InlineError } from "../components/InlineError";
import { Badge } from "../components/ui/badge";
import { Button } from "../components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "../components/ui/card";
import { Input } from "../components/ui/input";
import { Skeleton } from "../components/ui/skeleton";
import { HelpTooltip, InfoLabel, Tooltip, TooltipContent, TooltipTrigger } from "../components/ui/tooltip";
import { useSystemHealth } from "../hooks/use-live-query";
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
  const [retentionMaxAgeDays, setRetentionMaxAgeDays] = useState<number | undefined>(undefined);
  const [skillVersionRetentionDays, setSkillVersionRetentionDays] = useState<number | undefined>(undefined);
  const [complianceHardPurge, setComplianceHardPurge] = useState(false);
  const [complianceMode, setComplianceMode] = useState(false);
  const [eraseUserId, setEraseUserId] = useState("");
  const [confirmEraseUserId, setConfirmEraseUserId] = useState<string | null>(null);
  const [eraseNotice, setEraseNotice] = useState<string | null>(null);
  const [eraseError, setEraseError] = useState<string | null>(null);
  const [reindexResult, setReindexResult] = useState<{ enqueued: number; next_cursor: string | null } | null>(null);
  const [reindexError, setReindexError] = useState<string | null>(null);
  const [confirmReindex, setConfirmReindex] = useState(false);
  const embeddingModelTimeoutRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const llmModelTimeoutRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const importFileInputRef = useRef<HTMLInputElement | null>(null);
  const hasApiKey = apiKey.trim().length > 0;
  const canAct = hasApiKey && workspaceId.trim().length > 0;

  const healthQuery = useSystemHealth(canAct);

  // API Key state
  const [includeRevoked, setIncludeRevoked] = useState(false);
  const [newKeyName, setNewKeyName] = useState("");
  const [createdPlaintextKey, setCreatedPlaintextKey] = useState<string | null>(null);
  const [createdKeyName, setCreatedKeyName] = useState<string | null>(null);
  const [apiKeyError, setApiKeyError] = useState<string | null>(null);
  const [keyToRevoke, setKeyToRevoke] = useState<ApiKeySummary | null>(null);
  const [copied, setCopied] = useState(false);

  const keysQuery = useQuery({
    queryKey: ["workspace", workspaceId, "keys", includeRevoked],
    queryFn: () => listApiKeys(workspaceId, includeRevoked),
    enabled: hasApiKey && workspaceId.trim().length > 0,
  });

  const createKeyMutation = useMutation({
    mutationKey: ["workspace", workspaceId, "create-key"],
    mutationFn: (name: string) => createApiKey(workspaceId, name),
    onSuccess: (result) => {
      setApiKeyError(null);
      setCreatedPlaintextKey(result.plaintext_key);
      setCreatedKeyName(newKeyName);
      setNewKeyName("");
      void queryClient.invalidateQueries({ queryKey: ["workspace", workspaceId, "keys"] });
    },
    onError: (error: Error) => {
      setApiKeyError(error.message);
    },
  });

  const revokeKeyMutation = useMutation({
    mutationKey: ["workspace", workspaceId, "revoke-key"],
    mutationFn: (keyId: string) => revokeApiKey(workspaceId, keyId),
    onSuccess: () => {
      setApiKeyError(null);
      setKeyToRevoke(null);
      void queryClient.invalidateQueries({ queryKey: ["workspace", workspaceId, "keys"] });
    },
    onError: (error: Error) => {
      setApiKeyError(error.message);
      setKeyToRevoke(null);
    },
  });

  function handleCopy(text: string) {
    void navigator.clipboard.writeText(text);
    setCopied(true);
    setTimeout(() => setCopied(false), 2000);
  }

  function formatKeyDate(value: string | null | undefined): string {
    if (!value) return "Never";
    const date = new Date(value);
    if (Number.isNaN(date.getTime())) return "Never";
    return date.toLocaleDateString(undefined, {
      year: "numeric",
      month: "short",
      day: "numeric",
      hour: "2-digit",
      minute: "2-digit",
    });
  }

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
      setRetentionMaxAgeDays(workspace.retention_max_age_days ?? undefined);
      setSkillVersionRetentionDays(workspace.skill_version_retention_days ?? undefined);
      setComplianceHardPurge(workspace.compliance_hard_purge ?? false);
      setComplianceMode(workspace.compliance_mode ?? false);
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
  const forgetUserMutation = useMutation({
    mutationKey: ["workspace", workspaceId, "forget-user"],
    mutationFn: (userId: string) => forgetUserData(workspaceId, userId),
    onSuccess: (result: ForgetUserDataResponse) => {
      setEraseError(null);
      setEraseNotice(`Erased ${result.memories_purged} memories for ${result.user_id}`);
      setEraseUserId("");
      setConfirmEraseUserId(null);
      void queryClient.invalidateQueries({ queryKey: ["workspace", workspaceId, "memory"] });
      void queryClient.invalidateQueries({ queryKey: ["workspace", workspaceId, "settings"] });
    },
    onError: (error: Error) => {
      setEraseNotice(null);
      setEraseError(error.message);
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
      setRetentionMaxAgeDays(workspaceQuery.data.retention_max_age_days ?? undefined);
      setSkillVersionRetentionDays(workspaceQuery.data.skill_version_retention_days ?? undefined);
      setComplianceHardPurge(workspaceQuery.data.compliance_hard_purge ?? false);
      setComplianceMode(workspaceQuery.data.compliance_mode ?? false);
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

  function saveRetentionMaxAgeDays(value: string) {
    const trimmed = value.trim();
    if (trimmed.length === 0) {
      setRetentionMaxAgeDays(undefined);
      configMutation.mutate({ retention_max_age_days: null });
      return;
    }

    const days = Number(trimmed);
    if (!Number.isInteger(days) || days < 1 || days > 3650) {
      setConfigError("retention_max_age_days must be between 1 and 3650");
      return;
    }

    setConfigError(null);
    setRetentionMaxAgeDays(days);
    configMutation.mutate({ retention_max_age_days: days });
  }

  function toggleComplianceHardPurge() {
    const next = !complianceHardPurge;
    setComplianceHardPurge(next);
    configMutation.mutate({ compliance_hard_purge: next });
  }

  function saveSkillVersionRetentionDays(value: string) {
    const trimmed = value.trim();
    if (trimmed.length === 0) {
      setSkillVersionRetentionDays(undefined);
      configMutation.mutate({ skill_version_retention_days: null });
      return;
    }

    const days = Number(trimmed);
    if (!Number.isInteger(days) || days < 1 || days > 3650) {
      setConfigError("skill_version_retention_days must be between 1 and 3650");
      return;
    }

    setConfigError(null);
    setSkillVersionRetentionDays(days);
    configMutation.mutate({ skill_version_retention_days: days });
  }

  function toggleComplianceMode() {
    const next = !complianceMode;
    setComplianceMode(next);
    configMutation.mutate({ compliance_mode: next });
  }

  function requestEraseUserData() {
    const userId = eraseUserId.trim();
    if (userId.length === 0) {
      setEraseNotice(null);
      setEraseError("Enter a user_id to erase.");
      return;
    }

    setEraseError(null);
    setConfirmEraseUserId(userId);
  }

  function confirmEraseUserData() {
    if (!confirmEraseUserId) {
      return;
    }

    forgetUserMutation.mutate(confirmEraseUserId);
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
        <div className="flex items-center gap-2">
          <Badge variant={hasApiKey ? "green" : "amber"}>
            <ShieldCheck className="mr-1 h-3 w-3" aria-hidden="true" />
            {hasApiKey ? "API key loaded" : "Setup needed"}
          </Badge>
          <HelpTooltip label={hasApiKey ? "API key loaded" : "Setup needed"}>Shows whether this Control Center session has the credentials required to manage the active workspace.</HelpTooltip>
        </div>
      </header>

      <section className="grid gap-4 lg:grid-cols-[1fr_1fr]">
        <Card>
          <CardHeader className="flex flex-row items-center justify-between space-y-0">
            <CardTitle>Workspace</CardTitle>
            <KeyRound className="h-4 w-4 text-accent-strong" aria-hidden="true" />
          </CardHeader>
          <CardContent className="space-y-4">
            <div className="grid gap-3">
              <Field label="Workspace ID" helpText="Workspace identifier used to scope API calls, retrieval, and lifecycle operations." value={workspaceId} />
              <Field label="Key prefix" helpText="Visible prefix of the loaded API key so you can verify which credential is active without exposing the full secret." value={apiKey.slice(0, 8)} />
            </div>
          </CardContent>
        </Card>

        <Card>
          <CardHeader className="flex flex-row items-center justify-between space-y-0">
            <CardTitle className="flex items-center gap-1.5">
              <span>Backup</span>
              <HelpTooltip label="Backup">Export or restore workspace memories without changing backend APIs or lifecycle rules.</HelpTooltip>
            </CardTitle>
            <Download className="h-4 w-4 text-accent-strong" aria-hidden="true" />
          </CardHeader>
          <CardContent className="space-y-3">
            <div className="flex flex-wrap gap-2">
              <Tooltip>
                <TooltipTrigger asChild>
                  <Button
                    type="button"
                    data-testid="export-jsonl-button"
                    onClick={() => exportMutation.mutate()}
                    disabled={!hasApiKey || workspaceId.trim().length === 0 || exportMutation.isPending}
                  >
                    {exportMutation.isPending ? <Loader2 className="h-4 w-4 animate-spin" aria-hidden="true" /> : <Download className="h-4 w-4" aria-hidden="true" />}
                    Export JSONL
                  </Button>
                </TooltipTrigger>
                <TooltipContent>Downloads workspace memories as newline-delimited JSON for backup or migration.</TooltipContent>
              </Tooltip>
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
              <Tooltip>
                <TooltipTrigger asChild>
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
                </TooltipTrigger>
                <TooltipContent>Restores memories from a previous JSONL export.</TooltipContent>
              </Tooltip>
            </div>
            {importResult ? (
              <p className="text-sm text-ink/70">
                Imported {importResult.imported}; skipped {importResult.skipped}; errors {importResult.errors}
              </p>
            ) : null}
          </CardContent>
        </Card>
      </section>

      {/* Workspace API Keys Card */}
      <Card>
        <CardHeader className="flex flex-row items-center justify-between space-y-0">
          <CardTitle className="flex items-center gap-1.5">
            <span>Workspace API Keys</span>
            <HelpTooltip label="Workspace API Keys">Manage API keys used by external agents or CLI tools to connect to this workspace.</HelpTooltip>
          </CardTitle>
          <KeyRound className="h-4 w-4 text-accent-strong" aria-hidden="true" />
        </CardHeader>
        <CardContent className="grid gap-5">
          <div className="flex flex-col gap-4 sm:flex-row sm:items-center sm:justify-between">
            <p className="text-sm text-ink/75">
              API keys allow external tools (like the VS Code extension, cursor, or custom agents) to securely connect to MemoryOps.
            </p>
            <div className="flex items-center gap-2">
              <input
                id="show-revoked-keys"
                type="checkbox"
                checked={includeRevoked}
                onChange={(e) => setIncludeRevoked(e.target.checked)}
                className="h-4 w-4 rounded border-line text-accent focus:ring-accent"
              />
              <label htmlFor="show-revoked-keys" className="text-sm text-ink/70 cursor-pointer select-none">
                Show revoked keys
              </label>
            </div>
          </div>

          {apiKeyError ? <InlineError title="API Key action failed" message={apiKeyError} /> : null}

          {keysQuery.isLoading ? (
            <Skeleton className="h-32 w-full" />
          ) : keysQuery.isError ? (
            <InlineError title="Failed to load keys" message={keysQuery.error.message} />
          ) : keysQuery.data && keysQuery.data.length === 0 ? (
            <div className="text-center py-6 text-sm text-ink/55 border border-dashed border-line rounded-md">
              No API keys found. Generate a key below to get started.
            </div>
          ) : (
            <div className="thin-scrollbar overflow-auto rounded-md border border-line">
              <table className="w-full min-w-[640px] border-collapse text-left text-sm">
                <thead className="bg-soft text-xs uppercase text-ink/55">
                  <tr>
                    <th className="px-3 py-2 font-medium">Name</th>
                    <th className="px-3 py-2 font-medium">Prefix</th>
                    <th className="px-3 py-2 font-medium">Created</th>
                    <th className="px-3 py-2 font-medium">Last Used</th>
                    <th className="px-3 py-2 font-medium">Status</th>
                    <th className="px-3 py-2 font-medium text-right">Actions</th>
                  </tr>
                </thead>
                <tbody>
                  {keysQuery.data?.map((key) => {
                    const isCurrent = apiKey.startsWith(key.prefix);
                    return (
                      <tr key={key.id} className="border-t border-line align-middle hover:bg-soft/10">
                        <td className="px-3 py-3 text-ink font-medium">{key.name}</td>
                        <td className="px-3 py-3 font-mono text-xs text-ink/70">{key.prefix}</td>
                        <td className="px-3 py-3 text-ink/70">{formatKeyDate(key.created_at)}</td>
                        <td className="px-3 py-3 text-ink/70">{formatKeyDate(key.last_used_at)}</td>
                        <td className="px-3 py-3">
                          {key.revoked ? (
                            <Badge variant="rust">Revoked</Badge>
                          ) : isCurrent ? (
                            <Badge variant="blue">Current Key</Badge>
                          ) : (
                            <Badge variant="green">Active</Badge>
                          )}
                        </td>
                        <td className="px-3 py-3 text-right">
                          {!key.revoked && (
                            <Button
                              type="button"
                              variant="destructive"
                              size="sm"
                              disabled={isCurrent || revokeKeyMutation.isPending}
                              onClick={() => setKeyToRevoke(key)}
                            >
                              Revoke
                            </Button>
                          )}
                        </td>
                      </tr>
                    );
                  })}
                </tbody>
              </table>
            </div>
          )}

          <div className="grid gap-3 rounded-lg border border-line bg-soft/40 p-4">
            <div>
              <p className="text-sm font-medium text-ink">Generate New API Key</p>
              <p className="mt-1 text-xs text-ink/60">Create a new secure key for an agent or client integration.</p>
            </div>
            <form
              onSubmit={(e) => {
                e.preventDefault();
                const name = newKeyName.trim();
                if (name.length > 0) {
                  createKeyMutation.mutate(name);
                }
              }}
              className="flex flex-col gap-2 sm:flex-row"
            >
              <Input
                value={newKeyName}
                onChange={(e) => setNewKeyName(e.target.value)}
                placeholder="Key name (e.g. VS Code, Claude Code)"
                disabled={!canAct || createKeyMutation.isPending}
                required
              />
              <Button
                type="submit"
                disabled={!canAct || newKeyName.trim().length === 0 || createKeyMutation.isPending}
              >
                {createKeyMutation.isPending ? <Loader2 className="h-4 w-4 animate-spin" /> : "Generate"}
              </Button>
            </form>
          </div>
        </CardContent>
      </Card>

      <Card>
        <CardHeader className="flex flex-row items-center justify-between space-y-0">
          <CardTitle className="flex items-center gap-1.5">
            <span>Promotion</span>
            <HelpTooltip label="Promotion">Lifecycle controls for turning recurring episodic activity into durable semantic memory.</HelpTooltip>
          </CardTitle>
          <GitMerge className="h-4 w-4 text-accent-strong" aria-hidden="true" />
        </CardHeader>
        <CardContent className="grid gap-5">
          <div className="grid gap-5 xl:grid-cols-2">
            <label className="grid gap-2 text-sm text-ink/70">
              <span className="flex justify-between text-xs font-medium uppercase text-ink/45">
                <InfoLabel label={`Promotion threshold: ${promotionThreshold.toFixed(2)}`} tooltip="Minimum confidence required before related episodic memories are promoted into semantic memory." />
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
                <InfoLabel label={`Dedup cosine threshold: ${dedupThreshold.toFixed(2)}`} tooltip="Similarity cutoff used to avoid creating duplicate semantic memories." />
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
                <InfoLabel label={`Decay half-life: ${decayHalfLife}d`} tooltip="Number of days before normal memory priority decays by roughly half." />
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
                <InfoLabel label={`Prune below: ${pruningThreshold.toFixed(2)}`} tooltip="Memories below this decay or priority threshold may be pruned or archived during lifecycle processing." />
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
            <Tooltip>
              <TooltipTrigger asChild>
                <Button
                  type="button"
                  data-testid="run-promotion-button"
                  onClick={() => promotionMutation.mutate()}
                  disabled={!hasApiKey || workspaceId.trim().length === 0 || promotionMutation.isPending}
                >
                  {promotionMutation.isPending ? <Loader2 className="h-4 w-4 animate-spin" aria-hidden="true" /> : <Play className="h-4 w-4" aria-hidden="true" />}
                  Run Promotion Now
                </Button>
              </TooltipTrigger>
              <TooltipContent>Manually starts a lifecycle pass to cluster episodic memories and promote durable knowledge.</TooltipContent>
            </Tooltip>
            {promotionResult ? (
              <p className="text-sm text-ink/70">Promoted {promotionResult.units_promoted} semantic memories from {promotionResult.clusters_found} clusters</p>
            ) : null}
            {promotionError ? <InlineError title="Promotion failed" message={promotionError} /> : null}
            {configError ? <InlineError title="Config update failed" message={configError} /> : null}
          </div>

          <div className="grid gap-2">
            <label className="text-xs font-medium uppercase text-ink/45" htmlFor="sub-agent-pools">
              <InfoLabel label="Sub-Agent Pools" tooltip="Comma-separated agent pool names allowed to share memory within this workspace." />
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
          <CardTitle className="flex items-center gap-1.5">
            <span>Contradiction Detection</span>
            <HelpTooltip label="Contradiction Mode">Controls whether new contradictions wait for human review or resolve automatically.</HelpTooltip>
          </CardTitle>
          <ShieldAlert className="h-4 w-4 text-accent-strong" aria-hidden="true" />
        </CardHeader>
        <CardContent className="grid gap-4">
          <div className="flex items-start justify-between gap-4">
            <div>
              <p className="text-sm font-medium text-ink">
                <InfoLabel label="Contradiction Mode" tooltip="Choose whether contradictions are quarantined for review or auto-resolved by recency." />
              </p>
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
            <span className={contradictionMode === "quarantine" ? "font-semibold text-accent-strong" : ""}>
              Quarantine for review
            </span>
            <HelpTooltip label="Quarantine for review">New contradictions are flagged for manual operator review before any memory is archived.</HelpTooltip>
            <span>/</span>
            <span className={contradictionMode === "auto_resolve" ? "font-semibold text-accent-strong" : ""}>
              Auto-resolve newer wins
            </span>
            <HelpTooltip label="Auto-resolve newer wins">When a contradiction is detected, MemoryOps keeps the newer memory and archives the older one automatically.</HelpTooltip>
          </div>
        </CardContent>
      </Card>

      <Card>
        <CardHeader className="flex flex-row items-center justify-between space-y-0">
          <CardTitle className="flex items-center gap-1.5">
            <span>Provider config</span>
            <HelpTooltip label="Provider config">Embedding and LLM settings that control indexing, retrieval, and model-assisted workflows.</HelpTooltip>
          </CardTitle>
          {configMutation.isPending ? <Loader2 className="h-4 w-4 animate-spin text-accent-strong" aria-hidden="true" /> : <ServerCog className="h-4 w-4 text-accent-strong" aria-hidden="true" />}
        </CardHeader>
        <CardContent className="grid gap-4 sm:grid-cols-2">
          <div className="grid gap-4">
            <div className="grid gap-2">
              <label className="text-xs font-medium uppercase text-ink/45" htmlFor="embedding-provider">
                <InfoLabel label="Embedding provider" tooltip="Provider used to generate embeddings for retrieval indexing." />
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
                <InfoLabel label="Embedding model" tooltip="Specific embedding model used to vectorize workspace memories." />
              </label>
              <Input id="embedding-model" data-testid="embedding-model-input" value={embeddingModel} onChange={(event) => saveEmbeddingModel(event.target.value)} />
            </div>
          </div>

          <div className="grid gap-4">
            <div className="grid gap-2">
              <label className="text-xs font-medium uppercase text-ink/45" htmlFor="llm-provider">
                <InfoLabel label="LLM provider" tooltip="Provider used for model-assisted MemoryOps workflows." />
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
                <InfoLabel label="LLM model" tooltip="Specific language model used for model-assisted MemoryOps workflows." />
              </label>
              <Input id="llm-model" data-testid="llm-model-input" value={llmModel} onChange={(event) => saveLlmModel(event.target.value)} />
            </div>
          </div>
        </CardContent>
      </Card>

      {/* Re-Index ──────────────────────────────────────────────────────── */}
      <Card>
        <CardHeader className="flex flex-row items-center justify-between space-y-0">
          <CardTitle className="flex items-center gap-1.5">
            <span>Embedding Re-Index</span>
            <HelpTooltip label="Re-Index Workspace">Clears and rebuilds embeddings using the current embedding provider and model. Required after provider or model changes.</HelpTooltip>
          </CardTitle>
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
            <Tooltip>
              <TooltipTrigger asChild>
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
              </TooltipTrigger>
              <TooltipContent>Clears and rebuilds embeddings using the current embedding provider and model. Required after provider or model changes.</TooltipContent>
            </Tooltip>
          )}
        </CardContent>
      </Card>

      {/* System Health ─────────────────────────────────────────────────── */}
      <Card>
        <CardHeader className="flex flex-row items-center justify-between space-y-0">
          <CardTitle className="flex items-center gap-1.5">
            <span>System Health</span>
            <HelpTooltip label="System Health">Current readiness of the services MemoryOps depends on, such as database, queue, and vector index components.</HelpTooltip>
          </CardTitle>
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
                <Tooltip>
                  <TooltipTrigger asChild>
                    <Button
                      type="button"
                      variant="secondary"
                      size="sm"
                      onClick={() => void healthQuery.refetch()}
                      disabled={healthQuery.isFetching}
                    >
                      Check Now
                    </Button>
                  </TooltipTrigger>
                  <TooltipContent>Refreshes the latest system health checks for this workspace environment.</TooltipContent>
                </Tooltip>
              </div>
            </>
          ) : !healthQuery.isFetching ? (
            <p className="text-sm text-ink/55">Connect to see system health.</p>
          ) : null}
        </CardContent>
      </Card>

      {/* Compliance ───────────────────────────────────────────────────── */}
      <Card>
        <CardHeader className="flex flex-row items-center justify-between space-y-0">
          <div>
            <CardTitle className="flex items-center gap-1.5">
              <span>Compliance</span>
              <HelpTooltip label="Compliance settings">Retention, purge, and right-to-erasure controls for regulated memory handling.</HelpTooltip>
            </CardTitle>
            <p className="mt-1 text-xs text-ink/60">Data retention and right-to-erasure settings</p>
          </div>
          {configMutation.isPending ? <Loader2 className="h-4 w-4 animate-spin text-accent-strong" aria-hidden="true" /> : <Shield className="h-4 w-4 text-accent-strong" aria-hidden="true" />}
        </CardHeader>
        <CardContent className="grid gap-5">
          <div className="grid gap-4 lg:grid-cols-[1fr_auto] lg:items-start">
            <div>
              <p className="text-sm font-medium text-ink">
                <InfoLabel label="Retention max age days" tooltip="Maximum age in days before old memories become eligible for retention purge." />
              </p>
              <p className="mt-1 text-xs text-ink/60">Hard-purge memories older than this many days. Leave blank to disable.</p>
              {retentionMaxAgeDays ? (
                <Badge variant="amber" className="mt-2">
                  Active — memories older than {retentionMaxAgeDays} days will be purged daily
                </Badge>
              ) : null}
            </div>
            <Input
              data-testid="retention-max-age-days-input"
              type="number"
              min={1}
              max={3650}
              placeholder="No limit"
              value={retentionMaxAgeDays ?? ""}
              onChange={(event) => saveRetentionMaxAgeDays(event.target.value)}
              disabled={!canAct || configMutation.isPending}
              className="w-full lg:w-48"
            />
          </div>

          <div className="grid gap-4 lg:grid-cols-[1fr_auto] lg:items-start">
            <div>
              <p className="text-sm font-medium text-ink">
                <InfoLabel label="Skill version retention days" tooltip="Maximum age in days before historical skill versions are pruned. The latest snapshot per skill is always kept." />
              </p>
              <p className="mt-1 text-xs text-ink/60">Prune older skill snapshots daily while preserving the newest version record for each skill. Leave blank to disable.</p>
              {skillVersionRetentionDays ? (
                <Badge variant="teal" className="mt-2">
                  Active — historical skill versions older than {skillVersionRetentionDays} days will be pruned daily
                </Badge>
              ) : null}
            </div>
            <Input
              data-testid="skill-version-retention-days-input"
              type="number"
              min={1}
              max={3650}
              placeholder="No limit"
              value={skillVersionRetentionDays ?? ""}
              onChange={(event) => saveSkillVersionRetentionDays(event.target.value)}
              disabled={!canAct || configMutation.isPending}
              className="w-full lg:w-48"
            />
          </div>

          <div className="flex items-start justify-between gap-4">
            <div>
              <p className="text-sm font-medium text-ink">
                <InfoLabel label="Compliance hard purge" tooltip="When enabled, compliance deletion permanently removes matching data instead of soft-deleting it. Treat as destructive." />
              </p>
              <p className="mt-1 text-xs text-ink/60">Also delete originating webhook events on erasure and retention purge. Cannot be undone.</p>
            </div>
            <button
              type="button"
              role="switch"
              aria-checked={complianceHardPurge}
              data-testid="compliance-hard-purge-toggle"
              onClick={toggleComplianceHardPurge}
              disabled={!canAct || configMutation.isPending}
              className={cn(
                "relative inline-flex h-6 w-11 shrink-0 cursor-pointer rounded-full border-2 border-transparent transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent disabled:cursor-not-allowed disabled:opacity-50",
                complianceHardPurge ? "bg-accent" : "bg-ink/20",
              )}
            >
              <span
                className={cn(
                  "pointer-events-none inline-block h-5 w-5 rounded-full bg-white shadow-lg ring-0 transition-transform",
                  complianceHardPurge ? "translate-x-5" : "translate-x-0",
                )}
              />
            </button>
          </div>

          <div className="flex items-start justify-between gap-4">
            <div>
              <p className="text-sm font-medium text-ink">
                <InfoLabel label="Compliance mode" tooltip="Require an explicit change note for skill create, update, and rollback operations." />
              </p>
              <p className="mt-1 text-xs text-ink/60">When enabled, skill mutations must include a non-empty change note for auditability.</p>
            </div>
            <button
              type="button"
              role="switch"
              aria-checked={complianceMode}
              data-testid="compliance-mode-toggle"
              onClick={toggleComplianceMode}
              disabled={!canAct || configMutation.isPending}
              className={cn(
                "relative inline-flex h-6 w-11 shrink-0 cursor-pointer rounded-full border-2 border-transparent transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent disabled:cursor-not-allowed disabled:opacity-50",
                complianceMode ? "bg-accent" : "bg-ink/20",
              )}
            >
              <span
                className={cn(
                  "pointer-events-none inline-block h-5 w-5 rounded-full bg-white shadow-lg ring-0 transition-transform",
                  complianceMode ? "translate-x-5" : "translate-x-0",
                )}
              />
            </button>
          </div>

          <div className="grid gap-3 rounded-lg border border-line bg-soft/40 p-4">
            <div>
              <p className="text-sm font-medium text-ink">
                <InfoLabel label="Forget or erase user data" tooltip="Operator control to delete memories associated with a specific user scope." />
              </p>
              <p className="mt-1 text-xs text-ink/60">Hard-purge all memories for a specific user ID (GDPR Article 17 / CCPA). Cannot be undone.</p>
            </div>
            {eraseNotice ? (
              <div className="flex items-center gap-2 rounded-md border border-green-200 bg-green-50 px-3 py-2 text-sm text-green-700" role="status">
                <CheckCircle2 className="h-4 w-4" aria-hidden="true" />
                <span>{eraseNotice}</span>
              </div>
            ) : null}
            {eraseError ? <InlineError title="Erasure failed" message={eraseError} /> : null}
            <div className="flex flex-col gap-2 sm:flex-row">
              <Input
                data-testid="erase-user-id-input"
                value={eraseUserId}
                onChange={(event) => setEraseUserId(event.target.value)}
                placeholder="user_id to erase"
                disabled={!canAct || forgetUserMutation.isPending}
              />
              <Tooltip>
                <TooltipTrigger asChild>
                  <Button
                    type="button"
                    variant="destructive"
                    data-testid="erase-user-data-button"
                    onClick={requestEraseUserData}
                    disabled={!canAct || eraseUserId.trim().length === 0 || forgetUserMutation.isPending}
                  >
                    {forgetUserMutation.isPending ? <Loader2 className="h-4 w-4 animate-spin" aria-hidden="true" /> : <AlertTriangle className="h-4 w-4" aria-hidden="true" />}
                    Erase User Data
                  </Button>
                </TooltipTrigger>
                <TooltipContent>Deletes memories associated with the specified user scope. Treat as destructive.</TooltipContent>
              </Tooltip>
            </div>
            {confirmEraseUserId ? (
              <div className="rounded-lg border border-rust/30 bg-orange-50 p-3">
                <p className="text-sm font-medium text-ink">Erase all data for this user?</p>
                <p className="mt-1 text-sm text-ink/70">
                  This will {complianceHardPurge ? "permanently delete" : "soft-delete"} all memories scoped to '{confirmEraseUserId}'.
                </p>
                <div className="mt-3 flex justify-end gap-2">
                  <Button type="button" variant="ghost" size="sm" onClick={() => setConfirmEraseUserId(null)} disabled={forgetUserMutation.isPending}>
                    Cancel
                  </Button>
                  <Button type="button" variant="destructive" size="sm" onClick={confirmEraseUserData} disabled={forgetUserMutation.isPending}>
                    {forgetUserMutation.isPending ? <Loader2 className="h-3.5 w-3.5 animate-spin" aria-hidden="true" /> : <AlertTriangle className="h-3.5 w-3.5" aria-hidden="true" />}
                    Erase
                  </Button>
                </div>
              </div>
            ) : null}
          </div>
        </CardContent>
      </Card>

      {/* API Key Plaintext One-Time Modal */}
      {createdPlaintextKey && (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-xs p-4">
          <div className="w-full max-w-lg rounded-lg border border-line bg-white p-6 shadow-xl animate-in fade-in-50 zoom-in-95 duration-200">
            <h2 className="text-lg font-semibold text-ink flex items-center gap-2">
              <KeyRound className="h-5 w-5 text-accent-strong" />
              <span>API Key Generated Successfully</span>
            </h2>
            
            <div className="mt-4 rounded-lg border border-amber-200 bg-amber-50 p-4 text-sm text-amber-900 flex items-start gap-3">
              <AlertTriangle className="h-5 w-5 shrink-0 text-amber-600 mt-0.5" />
              <div>
                <p className="font-semibold">Copy this key now!</p>
                <p className="mt-1 text-xs opacity-90">
                  For security reasons, this key will only be displayed once. If you lose it, you will have to create a new one. It cannot be recovered later.
                </p>
              </div>
            </div>

            <div className="mt-4">
              <label className="text-xs font-semibold uppercase text-ink/45">Plaintext API Key</label>
              <div className="mt-1 flex items-center gap-2 rounded-md border border-line bg-soft p-3 font-mono text-sm break-all select-all">
                <span className="flex-1 select-all">{createdPlaintextKey}</span>
                <Button
                  type="button"
                  variant="secondary"
                  size="sm"
                  onClick={() => handleCopy(createdPlaintextKey)}
                >
                  <Clipboard className="mr-1.5 h-3.5 w-3.5" />
                  {copied ? "Copied" : "Copy"}
                </Button>
              </div>
            </div>

            <div className="mt-6 flex justify-end">
              <Button
                type="button"
                className="w-full sm:w-auto"
                onClick={() => {
                  setCreatedPlaintextKey(null);
                  setCreatedKeyName(null);
                }}
              >
                I have saved this key
              </Button>
            </div>
          </div>
        </div>
      )}

      {/* Revocation Confirmation Dialog */}
      {keyToRevoke && (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-xs p-4">
          <div className="w-full max-w-md rounded-lg border border-line bg-white p-6 shadow-xl animate-in fade-in-50 zoom-in-95 duration-200">
            <h2 className="text-lg font-semibold text-ink flex items-center gap-2">
              <AlertTriangle className="h-5 w-5 text-rust" />
              <span>Revoke API Key</span>
            </h2>
            <p className="mt-3 text-sm text-ink/75">
              Are you sure you want to revoke the API key <strong className="text-ink font-semibold">"{keyToRevoke.name}"</strong> (prefix: <code className="font-mono bg-soft px-1 rounded">{keyToRevoke.prefix}</code>)?
            </p>
            <p className="mt-2 text-xs text-rust font-medium">
              This action is immediate and permanent. Any agents or services using this key will be disconnected and unable to authenticate.
            </p>
            <div className="mt-6 flex justify-end gap-3">
              <Button
                type="button"
                variant="secondary"
                onClick={() => setKeyToRevoke(null)}
                disabled={revokeKeyMutation.isPending}
              >
                Cancel
              </Button>
              <Button
                type="button"
                variant="destructive"
                onClick={() => revokeKeyMutation.mutate(keyToRevoke.id)}
                disabled={revokeKeyMutation.isPending}
              >
                {revokeKeyMutation.isPending ? <Loader2 className="h-3.5 w-3.5 animate-spin" /> : "Revoke Key"}
              </Button>
            </div>
          </div>
        </div>
      )}
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

function Field({ label, helpText, value }: { label: string; helpText: string; value: string }) {
  return (
    <div>
      <p className="text-xs font-medium uppercase text-ink/45">
        <InfoLabel label={label} tooltip={helpText} />
      </p>
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
