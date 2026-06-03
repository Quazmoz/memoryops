import { Send, Sparkles } from "lucide-react";
import { useMutation } from "@tanstack/react-query";
import { useEffect, useMemo, useState } from "react";
import { Link } from "react-router-dom";

import {
  fireWebhook as submitWebhook,
  firstFixtureForSource,
  fixturesForSource,
  webhookSources,
  type WebhookFixture,
  type WebhookFixtureKind,
  type WebhookSource,
} from "../api/ingest";
import type { IngestResult, JsonValue } from "../api/types";
import { EmptyState } from "../components/EmptyState";
import { InlineError } from "../components/InlineError";
import { Badge } from "../components/ui/badge";
import { Button } from "../components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "../components/ui/card";
import { HelpTooltip, InfoLabel, Tooltip, TooltipContent, TooltipTrigger } from "../components/ui/tooltip";
import { cn } from "../lib/utils";
import { useAppStore } from "../store/app-store";

const defaultSource: WebhookSource = "github";

export function Ingest() {
  const workspaceId = useAppStore((state) => state.workspaceId);
  const [selectedSource, setSelectedSource] = useState<WebhookSource>(defaultSource);
  const [selectedKind, setSelectedKind] = useState<WebhookFixtureKind>(() => firstFixtureForSource(defaultSource).kind);
  const sourceFixtures = useMemo(() => fixturesForSource(selectedSource), [selectedSource]);
  const fixture = useMemo(() => sourceFixtures.find((option) => option.kind === selectedKind) ?? firstFixtureForSource(selectedSource), [selectedKind, selectedSource, sourceFixtures]);
  const [payloadText, setPayloadText] = useState(formatPayload(fixture.payload));
  const [parseError, setParseError] = useState<string | null>(null);
  const mutation = useMutation<IngestResult, Error, { fixture: WebhookFixture; payload: JsonValue }>({
    mutationKey: ["workspace", workspaceId, "ingest", selectedSource],
    mutationFn: ({ fixture: selectedFixture, payload }) => submitWebhook(workspaceId, selectedFixture, payload),
  });

  useEffect(() => {
    setPayloadText(formatPayload(fixture.payload));
    setParseError(null);
  }, [fixture]);

  function selectSource(source: WebhookSource) {
    if (source === selectedSource) {
      return;
    }

    const nextFixture = firstFixtureForSource(source);
    setSelectedSource(source);
    setSelectedKind(nextFixture.kind);
    setPayloadText(formatPayload(nextFixture.payload));
    setParseError(null);
    mutation.reset();
  }

  function handleFireWebhook() {
    setParseError(null);
    const parsed = parsePayload(payloadText);
    if ("error" in parsed) {
      setParseError(parsed.error);
      return;
    }

    mutation.reset();
    mutation.mutate({ fixture, payload: parsed.value });
  }

  const response = mutation.data;
  const accepted = response?.ok === true && response.status === 202;
  const acceptedEventId = response ? eventId(response) : null;
  const showExplorerLink = accepted && fixture.source === "github" && acceptedEventId !== null;

  return (
    <div className="mx-auto grid max-w-7xl gap-5">
      <header>
        <p className="text-sm font-medium text-accent-strong">Dev webhook console</p>
        <h1 className="mt-1 text-2xl font-semibold tracking-normal text-ink">Webhook Tester</h1>
      </header>

      <section className="grid gap-4 xl:grid-cols-[26rem_1fr]">
        <Card>
          <CardHeader className="pb-0">
            <CardTitle className="flex items-center gap-1.5">
              <span>Event</span>
              <HelpTooltip label="Event">Choose the source fixture and event shape you want MemoryOps to ingest.</HelpTooltip>
            </CardTitle>
            <div className="mt-3 flex overflow-x-auto thin-scrollbar border-b border-line" role="tablist" aria-label="Webhook source">
              {webhookSources.map((source) => {
                const active = selectedSource === source.source;
                return (
                  <Tooltip key={source.source}>
                    <TooltipTrigger asChild>
                      <button
                        type="button"
                        role="tab"
                        data-testid={`source-tab-${source.source}`}
                        aria-selected={active}
                        onClick={() => selectSource(source.source)}
                        className={cn(
                          "shrink-0 whitespace-nowrap border-b-2 px-3 pb-2 pt-1 text-sm font-medium transition-colors",
                          active ? "border-accent text-accent-strong" : "border-transparent text-ink/55 hover:border-line hover:text-ink",
                        )}
                      >
                        {source.label}
                      </button>
                    </TooltipTrigger>
                    <TooltipContent>Simulates a {source.label} webhook arriving at the MemoryOps ingest pipeline.</TooltipContent>
                  </Tooltip>
                );
              })}
            </div>
          </CardHeader>
          <CardContent className="grid gap-4">
            <label className="grid gap-2 text-sm font-medium text-ink/70">
              <InfoLabel label="Event type" tooltip="Specific webhook fixture to send for the selected integration source." />
              <select
                data-testid="webhook-event-select"
                value={selectedKind}
                onChange={(event) => setSelectedKind(event.target.value as WebhookFixtureKind)}
                className="h-10 rounded-md border border-line bg-white px-3 text-sm text-ink outline-none focus:border-accent focus:ring-2 focus:ring-accent/20"
              >
                {sourceFixtures.map((option) => (
                  <option key={option.kind} value={option.kind}>
                    {option.label}
                  </option>
                ))}
              </select>
            </label>

            <div className="rounded-md border border-line bg-soft p-3 text-sm">
              <p className="text-xs font-medium uppercase text-ink/45">
                <InfoLabel label="Actor" tooltip="Principal or system that appears to have produced the fixture event." />
              </p>
              <p className="mt-1 font-mono">{fixture.actor}</p>
            </div>

            <Tooltip>
              <TooltipTrigger asChild>
                <Button type="button" data-testid="fire-webhook-button" onClick={handleFireWebhook} disabled={mutation.isPending}>
                  <Send className="h-4 w-4" aria-hidden="true" />
                  {mutation.isPending ? "Firing" : "Fire Webhook"}
                </Button>
              </TooltipTrigger>
              <TooltipContent>Sends this fixture to the backend as if the selected integration delivered a webhook event.</TooltipContent>
            </Tooltip>
          </CardContent>
        </Card>

        <Card>
          <CardHeader>
            <CardTitle className="flex items-center gap-1.5">
              <span>Payload</span>
              <HelpTooltip label="Payload editor">Editable JSON fixture. Use this to test ingestion, extraction, memory creation, and error handling.</HelpTooltip>
            </CardTitle>
          </CardHeader>
          <CardContent className="grid gap-4">
            <textarea
              data-testid="webhook-payload"
              value={payloadText}
              onChange={(event) => setPayloadText(event.target.value)}
              spellCheck={false}
              className="thin-scrollbar min-h-[30rem] resize-y rounded-md border border-line bg-[#101714] p-4 font-mono text-xs leading-5 text-[#e8f1e9] outline-none focus:border-accent focus:ring-2 focus:ring-accent/20"
            />
            {parseError ? <InlineError title="Payload is not valid JSON" message={parseError} /> : null}
          </CardContent>
        </Card>
      </section>

      <Card>
        <CardHeader className="flex flex-row items-center justify-between space-y-0">
          <CardTitle className="flex items-center gap-1.5">
            <span>Response</span>
            <HelpTooltip label="Response panel">Raw backend response returned after the ingest request is accepted or rejected.</HelpTooltip>
          </CardTitle>
          <div className="flex items-center gap-2">
            <Badge variant="muted" className="capitalize">{fixture.source}</Badge>
            {response ? (
              <Tooltip>
                <TooltipTrigger asChild>
                  <Badge data-testid="webhook-response-status" variant={response.ok ? "accent" : "rust"} tabIndex={0} className="focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent">
                    {response.status}
                  </Badge>
                </TooltipTrigger>
                <TooltipContent>HTTP status returned by the backend for this ingest request.</TooltipContent>
              </Tooltip>
            ) : null}
          </div>
        </CardHeader>
        <CardContent>
          {!response && !mutation.isPending ? (
            <EmptyState title="Ready to fire" message="The selected fixture will send through the Vite proxy to the live backend." />
          ) : null}
          {mutation.isPending ? (
            <div className="rounded-lg border border-line bg-soft p-5 text-sm text-ink/65">Sending fixture...</div>
          ) : null}
          {mutation.isError ? <InlineError message={mutation.error.message} /> : null}
          {response && !response.ok ? <InlineError title="Webhook rejected" message={response.detail ?? "The backend returned an error."} /> : null}
          {response ? (
            <pre className="thin-scrollbar max-h-64 overflow-auto rounded-md border border-line bg-soft p-4 text-xs text-ink">{JSON.stringify(response.data, null, 2)}</pre>
          ) : null}
          {accepted ? (
            <div className="mt-4 flex flex-wrap items-center gap-3 rounded-lg border border-green-200 bg-green-50 p-4 text-sm text-green-800">
              <Sparkles className="h-4 w-4" aria-hidden="true" />
              <Tooltip>
                <TooltipTrigger asChild>
                  <span className="rounded-sm focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent" tabIndex={0}>
                    202 Accepted{acceptedEventId ? ` · ${acceptedEventId}` : ""}
                  </span>
                </TooltipTrigger>
                <TooltipContent>The backend accepted the event for asynchronous processing. Memories may appear after the processor handles it.</TooltipContent>
              </Tooltip>
              {showExplorerLink ? (
                <Tooltip>
                  <TooltipTrigger asChild>
                    <Button asChild variant="secondary" size="sm">
                      <Link to={`/memory?q=${encodeURIComponent(fixture.actor)}`} data-testid="view-in-explorer-link">
                        View in Explorer
                      </Link>
                    </Button>
                  </TooltipTrigger>
                  <TooltipContent>Jumps into Memory Explorer so you can look for memories created from this test event.</TooltipContent>
                </Tooltip>
              ) : null}
            </div>
          ) : null}
        </CardContent>
      </Card>
    </div>
  );
}

function formatPayload(payload: JsonValue): string {
  return JSON.stringify(payload, null, 2);
}

function parsePayload(value: string): { value: JsonValue; error?: never } | { value?: never; error: string } {
  try {
    const parsed = JSON.parse(value) as unknown;
    if (isJsonValue(parsed)) {
      return { value: parsed };
    }
    return { error: "The root value must be JSON-compatible." };
  } catch (error) {
    return { error: error instanceof Error ? error.message : "JSON parsing failed." };
  }
}

function isJsonValue(value: unknown): value is JsonValue {
  if (value === null || typeof value === "string" || typeof value === "number" || typeof value === "boolean") {
    return true;
  }

  if (Array.isArray(value)) {
    return value.every(isJsonValue);
  }

  if (typeof value === "object") {
    return Object.values(value).every(isJsonValue);
  }

  return false;
}

function eventId(response: IngestResult): string | null {
  const data = response.data;
  if (typeof data === "object" && data !== null && !Array.isArray(data) && "event_id" in data && typeof data.event_id === "string") {
    return data.event_id;
  }

  return null;
}
