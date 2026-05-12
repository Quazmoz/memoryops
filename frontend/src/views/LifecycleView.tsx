import { Database, GitMerge, Loader2, Play, SlidersHorizontal } from "lucide-react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useEffect, useRef, useState } from "react";

import type { MemoryUnit, PromotionReport, WorkspaceConfig } from "../api/types";
import { getWorkspace, triggerPromotion, updateWorkspaceConfig } from "../api/workspaces";
import { EmptyState } from "../components/EmptyState";
import { InlineError } from "../components/InlineError";
import { Badge } from "../components/ui/badge";
import { Button } from "../components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "../components/ui/card";
import { Skeleton } from "../components/ui/skeleton";
import { HelpTooltip, InfoLabel, Tooltip, TooltipContent, TooltipTrigger } from "../components/ui/tooltip";
import { useMemoryList } from "../hooks/use-memory";
import { formatCount, formatDateTime, formatRelativeTime, previewText } from "../lib/format";
import { useAppStore } from "../store/app-store";

type SavedThresholds = {
  promotion_threshold: number;
  dedup_cosine_threshold: number;
};

export function LifecycleView() {
  const workspaceId = useAppStore((state) => state.workspaceId);
  const apiKey = useAppStore((state) => state.apiKey);
  const authReady = workspaceId.trim().length > 0 && apiKey.trim().length > 0;
  const queryClient = useQueryClient();
  const [promotionThreshold, setPromotionThreshold] = useState(0.72);
  const [dedupThreshold, setDedupThreshold] = useState(0.92);
  const [configError, setConfigError] = useState<string | null>(null);
  const [promotionError, setPromotionError] = useState<string | null>(null);
  const [promotionReport, setPromotionReport] = useState<PromotionReport | null>(null);
  const lastSaved = useRef<SavedThresholds | null>(null);

  const workspaceQuery = useQuery({
    queryKey: ["workspace", workspaceId, "lifecycle"],
    queryFn: () => getWorkspace(workspaceId),
    enabled: authReady,
  });
  const semantic = useMemoryList(workspaceId, {
    limit: 20,
    offset: 0,
    memoryType: "semantic",
    sort: "updated_at",
    direction: "desc",
  });
  const configMutation = useMutation({
    mutationKey: ["workspace", workspaceId, "lifecycle-config"],
    mutationFn: (patch: WorkspaceConfig) => updateWorkspaceConfig(workspaceId, patch),
    onSuccess: (workspace) => {
      setConfigError(null);
      lastSaved.current = {
        promotion_threshold: workspace.promotion_threshold,
        dedup_cosine_threshold: workspace.dedup_cosine_threshold,
      };
      queryClient.setQueryData(["workspace", workspaceId, "lifecycle"], workspace);
    },
    onError: (error: Error) => setConfigError(error.message),
  });
  const promotionMutation = useMutation({
    mutationKey: ["workspace", workspaceId, "promotion"],
    mutationFn: () => triggerPromotion(workspaceId),
    onSuccess: (report) => {
      setPromotionError(null);
      setPromotionReport(report);
      void queryClient.invalidateQueries({ queryKey: ["workspace", workspaceId, "memory"] });
    },
    onError: (error: Error) => {
      setPromotionReport(null);
      setPromotionError(error.message);
    },
  });

  useEffect(() => {
    if (!workspaceQuery.data) {
      return;
    }

    const next = {
      promotion_threshold: workspaceQuery.data.promotion_threshold,
      dedup_cosine_threshold: workspaceQuery.data.dedup_cosine_threshold,
    };
    lastSaved.current = next;
    setPromotionThreshold(next.promotion_threshold);
    setDedupThreshold(next.dedup_cosine_threshold);
  }, [workspaceQuery.data]);

  useEffect(() => {
    if (!authReady || !lastSaved.current) {
      return;
    }

    const patch = changedThresholds(lastSaved.current, promotionThreshold, dedupThreshold);
    if (!patch) {
      return;
    }

    const timeoutId = window.setTimeout(() => {
      configMutation.mutate(patch);
    }, 500);

    return () => window.clearTimeout(timeoutId);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [authReady, promotionThreshold, dedupThreshold]);

  return (
    <div className="mx-auto grid max-w-7xl gap-6">
      <header className="flex flex-col gap-4 sm:flex-row sm:items-center sm:justify-between">
        <div>
          <p className="text-sm font-medium text-accent-strong">Promotion</p>
          <h1 className="mt-1 text-2xl font-semibold tracking-normal text-ink">Lifecycle</h1>
        </div>
        <div className="flex items-center gap-2">
          <Badge variant={authReady ? "green" : "amber"}>{authReady ? "Workspace connected" : "Setup needed"}</Badge>
          <HelpTooltip label={authReady ? "Workspace connected" : "Setup needed"}>
            Shows whether this workspace is configured well enough to run lifecycle and promotion operations.
          </HelpTooltip>
        </div>
      </header>

      {workspaceQuery.isError ? <InlineError title="Workspace unavailable" message={workspaceQuery.error.message} /> : null}
      {configError ? <InlineError title="Config update failed" message={configError} /> : null}
      {promotionError ? <InlineError title="Promotion failed" message={promotionError} /> : null}

      <Card>
        <CardHeader className="flex flex-row items-center justify-between space-y-0">
          <CardTitle className="flex items-center gap-1.5">
            <span>Promotion Controls</span>
            <HelpTooltip label="Promotion Controls">Tune how MemoryOps clusters episodic memories and promotes durable semantic knowledge.</HelpTooltip>
          </CardTitle>
          {configMutation.isPending ? <Loader2 className="h-4 w-4 animate-spin text-accent-strong" aria-hidden="true" /> : <GitMerge className="h-4 w-4 text-accent-strong" aria-hidden="true" />}
        </CardHeader>
        <CardContent className="grid gap-5 xl:grid-cols-[1fr_1fr_auto] xl:items-end">
          <ThresholdSlider
            label="promotion_threshold"
            helpText="Minimum confidence required before related episodic memories are promoted into semantic memory."
            min={0.5}
            max={1}
            value={promotionThreshold}
            onChange={setPromotionThreshold}
            disabled={!authReady || workspaceQuery.isLoading}
          />
          <ThresholdSlider
            label="dedup_cosine_threshold"
            helpText="Similarity cutoff used to avoid creating duplicate semantic memories."
            min={0.8}
            max={0.99}
            value={dedupThreshold}
            onChange={setDedupThreshold}
            disabled={!authReady || workspaceQuery.isLoading}
          />
          <Tooltip>
            <TooltipTrigger asChild>
              <Button type="button" data-testid="manual-promote-button" onClick={() => promotionMutation.mutate()} disabled={!authReady || promotionMutation.isPending}>
                {promotionMutation.isPending ? <Loader2 className="h-4 w-4 animate-spin" aria-hidden="true" /> : <Play className="h-4 w-4" aria-hidden="true" />}
                Run Promotion Pass
              </Button>
            </TooltipTrigger>
            <TooltipContent>Manually starts a lifecycle pass to cluster episodic memories and promote durable knowledge.</TooltipContent>
          </Tooltip>
        </CardContent>
      </Card>

      {promotionReport ? (
        <section className="grid gap-4 md:grid-cols-3" aria-label="Promotion report" data-testid="promote-status">
          <ReportCard title="clusters_found" helpText="Related episodic groups MemoryOps considered for promotion during this pass." value={promotionReport.clusters_found} />
          <ReportCard title="units_promoted" helpText="New semantic memories created from qualifying episodic clusters." value={promotionReport.units_promoted} />
          <ReportCard title="units_skipped" helpText="Candidate units skipped because they failed the confidence threshold or looked like duplicates." value={promotionReport.units_skipped} />
        </section>
      ) : null}

      <section className="grid gap-4">
        <div className="flex items-center justify-between">
          <div>
            <p className="text-sm font-medium text-accent-strong">Semantic memory</p>
            <h2 className="mt-1 inline-flex items-center gap-1.5 text-xl font-semibold tracking-normal text-ink">
              <span>Recent Promotions</span>
              <HelpTooltip label="Recent Promotions">Recently promoted semantic memories so you can inspect what durable knowledge lifecycle created.</HelpTooltip>
            </h2>
          </div>
          <Database className="h-4 w-4 text-accent-strong" aria-hidden="true" />
        </div>

        {semantic.isLoading ? <Skeleton className="h-48 w-full" /> : null}
        {semantic.isError ? <InlineError title="Semantic memories unavailable" message={semantic.error.message} /> : null}
        {!semantic.isLoading && !semantic.isError && (semantic.data?.items.length ?? 0) === 0 ? (
          <EmptyState title="No semantic memories yet" message="Promotion output will appear here after episodic clusters qualify." />
        ) : null}
        {!semantic.isLoading && semantic.data && semantic.data.items.length > 0 ? (
          <div className="grid gap-3">
            {semantic.data.items.map((memory) => (
              <SemanticMemoryCard key={memory.id} memory={memory} />
            ))}
          </div>
        ) : null}
      </section>
    </div>
  );
}

function ThresholdSlider({ label, helpText, min, max, value, onChange, disabled }: { label: string; helpText: string; min: number; max: number; value: number; onChange: (value: number) => void; disabled: boolean }) {
  return (
    <label className="grid gap-2 text-sm text-ink/70">
      <span className="flex justify-between text-xs font-medium uppercase text-ink/45">
        <InfoLabel label={label} tooltip={helpText} />
        <span className="inline-flex items-center gap-1 font-mono text-ink/70">
          <SlidersHorizontal className="h-3.5 w-3.5" aria-hidden="true" />
          {value.toFixed(2)}
        </span>
      </span>
      <input
        type="range"
        min={min}
        max={max}
        step="0.01"
        value={value}
        onChange={(event) => onChange(Number(event.target.value))}
        disabled={disabled}
        className="accent-accent disabled:opacity-50"
      />
    </label>
  );
}

function ReportCard({ title, helpText, value }: { title: string; helpText: string; value: number }) {
  return (
    <Card>
      <CardHeader className="pb-3">
        <CardTitle className="text-sm font-medium text-ink/65">
          <InfoLabel label={title} tooltip={helpText} />
        </CardTitle>
      </CardHeader>
      <CardContent>
        <p className="text-3xl font-semibold">{formatCount(value)}</p>
      </CardContent>
    </Card>
  );
}

function SemanticMemoryCard({ memory }: { memory: MemoryUnit }) {
  return (
    <Card>
      <CardContent className="grid gap-3 p-4 sm:grid-cols-[1fr_auto] sm:items-center">
        <div className="min-w-0">
          <p className="text-sm font-medium text-ink">{previewText(memory.content, 160)}</p>
          <div className="mt-2 flex flex-wrap items-center gap-2 text-xs text-ink/60">
            <Tooltip>
              <TooltipTrigger asChild>
                <Badge variant="teal" tabIndex={0} className="focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent">
                  source_count {sourceCount(memory)}
                </Badge>
              </TooltipTrigger>
              <TooltipContent>How many source episodes or events were consolidated into this semantic memory.</TooltipContent>
            </Tooltip>
            <span className="inline-flex items-center gap-1">
              <span>Promoted {memory.promoted_at ? formatRelativeTime(memory.promoted_at) : "Pending"}</span>
              <HelpTooltip label="promoted timestamp">When this semantic memory was promoted from episodic material.</HelpTooltip>
            </span>
          </div>
        </div>
        <span className="whitespace-nowrap text-xs text-ink/55">{formatDateTime(memory.updated_at)}</span>
      </CardContent>
    </Card>
  );
}

function changedThresholds(saved: SavedThresholds, promotionThreshold: number, dedupThreshold: number): WorkspaceConfig | null {
  const patch: WorkspaceConfig = {};

  if (Math.abs(saved.promotion_threshold - promotionThreshold) > 0.0001) {
    patch.promotion_threshold = promotionThreshold;
  }
  if (Math.abs(saved.dedup_cosine_threshold - dedupThreshold) > 0.0001) {
    patch.dedup_cosine_threshold = dedupThreshold;
  }

  return Object.keys(patch).length > 0 ? patch : null;
}

function sourceCount(memory: MemoryUnit): number {
  if (memory.source_episode_ids.length > 0) {
    return memory.source_episode_ids.length;
  }
  if (memory.source_events.length > 0) {
    return memory.source_events.length;
  }
  return memory.corroboration_count;
}
