import { ChevronDown, ChevronUp } from "lucide-react";
import { useEffect, useMemo, useState } from "react";

import type { MetricsValues } from "../api/metrics";
import { useMetrics } from "../hooks/useMetrics";
import { useAppStore } from "../store/app-store";
import { InlineError } from "./InlineError";
import { Card, CardContent, CardHeader, CardTitle } from "./ui/card";
import { Skeleton } from "./ui/skeleton";

const DASH = "—";

type MetricKey = keyof MetricsValues;

type MetricSpec = {
  key: MetricKey;
  label: string;
  formatter: (value: number) => string;
  highlightOnNonZero?: boolean;
};

const METRICS: MetricSpec[][] = [
  [
    { key: "ingest_events_total", label: "Ingest Events", formatter: formatCount },
    { key: "slow_path_jobs_processed", label: "Slow Path Processed", formatter: formatCount },
    {
      key: "slow_path_jobs_failed",
      label: "Slow Path Failed",
      formatter: formatCount,
      highlightOnNonZero: true,
    },
  ],
  [
    { key: "retrieval_requests_total", label: "Retrieval Requests", formatter: formatCount },
    { key: "embedding_latency_p50_ms", label: "Embed P50 ms", formatter: formatLatency },
    { key: "embedding_latency_p99_ms", label: "Embed P99 ms", formatter: formatLatency },
  ],
  [
    { key: "llm_latency_p50_ms", label: "LLM P50 ms", formatter: formatLatency },
    { key: "llm_latency_p99_ms", label: "LLM P99 ms", formatter: formatLatency },
    { key: "token_pack_budget_used_pct", label: "Token Budget Used %", formatter: formatPercent },
  ],
];

export function MetricsPanel() {
  const workspaceId = useAppStore((state) => state.workspaceId);
  const metrics = useMetrics(workspaceId);
  const [expanded, setExpanded] = useState(true);
  const [now, setNow] = useState(() => Date.now());

  useEffect(() => {
    const handle = window.setInterval(() => setNow(Date.now()), 1_000);
    return () => window.clearInterval(handle);
  }, []);

  const lastUpdatedLabel = useMemo(() => {
    if (!metrics.dataUpdatedAt) {
      return null;
    }
    const seconds = Math.max(0, Math.round((now - metrics.dataUpdatedAt) / 1000));
    return `Last updated: ${seconds}s ago`;
  }, [metrics.dataUpdatedAt, now]);

  return (
    <Card data-testid="metrics-panel">
      <CardHeader className="flex flex-row items-center justify-between space-y-0">
        <CardTitle>Telemetry</CardTitle>
        <div className="flex items-center gap-3 text-xs text-ink/60">
          {lastUpdatedLabel ? <span data-testid="metrics-last-updated">{lastUpdatedLabel}</span> : null}
          <button
            type="button"
            onClick={() => setExpanded((value) => !value)}
            className="inline-flex items-center gap-1 rounded-md px-2 py-1 hover:bg-ink/5"
            aria-expanded={expanded}
            aria-controls="metrics-panel-grid"
            data-testid="metrics-panel-toggle"
          >
            {expanded ? <ChevronUp className="h-4 w-4" aria-hidden="true" /> : <ChevronDown className="h-4 w-4" aria-hidden="true" />}
            <span>{expanded ? "Collapse" : "Expand"}</span>
          </button>
        </div>
      </CardHeader>
      {expanded ? (
        <CardContent id="metrics-panel-grid">
          {metrics.isError ? <InlineError title="Metrics unavailable" message={errorMessage(metrics.error)} /> : null}
          {metrics.isPending ? (
            <div className="grid grid-cols-1 gap-3 sm:grid-cols-3">
              {Array.from({ length: 9 }).map((_, index) => (
                <Skeleton className="h-16 w-full" key={index} data-testid="metrics-cell-skeleton" />
              ))}
            </div>
          ) : null}
          {!metrics.isPending && !metrics.isError ? (
            <div className="grid grid-cols-1 gap-3 sm:grid-cols-3">
              {METRICS.flat().map((spec) => (
                <MetricCell key={spec.key} spec={spec} value={metrics.data?.metrics[spec.key] ?? null} />
              ))}
            </div>
          ) : null}
        </CardContent>
      ) : null}
    </Card>
  );
}

function MetricCell({ spec, value }: { spec: MetricSpec; value: number | null }) {
  const isAlert = Boolean(spec.highlightOnNonZero && typeof value === "number" && value > 0);
  const valueText = typeof value === "number" ? spec.formatter(value) : DASH;

  return (
    <div className="rounded-md border border-line bg-soft p-3" data-testid={`metric-${spec.key}`}>
      <p className="text-xs text-ink/60">{spec.label}</p>
      <p className={`mt-1 text-xl font-mono tabular-nums ${isAlert ? "text-rust" : "text-ink"}`}>{valueText}</p>
    </div>
  );
}

function formatCount(value: number): string {
  return new Intl.NumberFormat("en", { maximumFractionDigits: 0 }).format(value);
}

function formatLatency(value: number): string {
  return value.toFixed(1);
}

function formatPercent(value: number): string {
  return `${value.toFixed(1)}%`;
}

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : "Metrics could not be loaded.";
}
