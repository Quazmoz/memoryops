import { extractDetail, parseResponse, requestHeaders } from "./client";
import type { IngestResult, JsonValue } from "./types";

export type WebhookEventKind =
  | "pull_request_opened"
  | "pull_request_merged"
  | "push"
  | "pull_request_review_approved"
  | "issue"
  | "issue_comment";

export type WebhookFixture = {
  kind: WebhookEventKind;
  label: string;
  githubEvent: "pull_request" | "push" | "pull_request_review" | "issues" | "issue_comment";
  actor: string;
  payload: JsonValue;
};

const now = "2026-04-27T15:20:30Z";

export const webhookFixtures: WebhookFixture[] = [
  {
    kind: "pull_request_opened",
    label: "pull_request (opened)",
    githubEvent: "pull_request",
    actor: "mona",
    payload: {
      action: "opened",
      sender: { login: "mona", type: "User" },
      repository: { full_name: "Quazmoz/memoryops", default_branch: "main" },
      pull_request: {
        number: 42,
        title: "Add retrieval score explanations",
        state: "open",
        merged: false,
        html_url: "https://github.com/Quazmoz/memoryops/pull/42",
        head: { ref: "feature/retrieval-score-notes" },
        base: { ref: "main" },
        updated_at: now,
        user: { login: "mona" },
      },
    },
  },
  {
    kind: "pull_request_merged",
    label: "pull_request (merged)",
    githubEvent: "pull_request",
    actor: "kai",
    payload: {
      action: "closed",
      sender: { login: "kai", type: "User" },
      repository: { full_name: "Quazmoz/memoryops", default_branch: "main" },
      pull_request: {
        number: 38,
        title: "Promote semantic memory clusters",
        state: "closed",
        merged: true,
        merged_at: now,
        html_url: "https://github.com/Quazmoz/memoryops/pull/38",
        head: { ref: "feature/promotion" },
        base: { ref: "main" },
        updated_at: now,
        user: { login: "kai" },
      },
    },
  },
  {
    kind: "push",
    label: "push",
    githubEvent: "push",
    actor: "nora",
    payload: {
      ref: "refs/heads/main",
      before: "9fceb02f9fceb02f9fceb02f9fceb02f9fceb02f",
      after: "b6fc4c620b67d95f953a5c1c1230aaab5db5a1b0",
      pusher: { name: "nora", email: "nora@example.com" },
      repository: { full_name: "Quazmoz/memoryops", pushed_at: 1777303230 },
      commits: [
        {
          id: "b6fc4c620b67d95f953a5c1c1230aaab5db5a1b0",
          message: "Wire memory explorer filters",
          timestamp: now,
          author: { name: "nora", email: "nora@example.com" },
        },
      ],
    },
  },
  {
    kind: "pull_request_review_approved",
    label: "pull_request_review (approved)",
    githubEvent: "pull_request_review",
    actor: "sasha",
    payload: {
      action: "submitted",
      sender: { login: "sasha", type: "User" },
      repository: { full_name: "Quazmoz/memoryops", default_branch: "main" },
      pull_request: { number: 42, title: "Add retrieval score explanations" },
      review: {
        state: "approved",
        body: "Looks good. The scoring notes are clear and operator-friendly.",
        submitted_at: now,
        user: { login: "sasha" },
      },
    },
  },
  {
    kind: "issue",
    label: "issue",
    githubEvent: "issues",
    actor: "jules",
    payload: {
      action: "opened",
      sender: { login: "jules", type: "User" },
      repository: { full_name: "Quazmoz/memoryops", default_branch: "main" },
      issue: {
        number: 77,
        title: "Trace filters should include agent scope",
        body: "Operators need to separate workspace-wide traces from agent-specific lookups.",
        state: "open",
        updated_at: now,
        user: { login: "jules" },
      },
    },
  },
  {
    kind: "issue_comment",
    label: "issue_comment",
    githubEvent: "issue_comment",
    actor: "riley",
    payload: {
      action: "created",
      sender: { login: "riley", type: "User" },
      repository: { full_name: "Quazmoz/memoryops", default_branch: "main" },
      issue: { number: 77, title: "Trace filters should include agent scope" },
      comment: {
        body: "Let's tag these memories with agent:retrieval-worker while M6 is in flight.",
        updated_at: now,
        user: { login: "riley" },
      },
    },
  },
];

export function fixtureFor(kind: WebhookEventKind): WebhookFixture {
  const fallback = webhookFixtures[0];
  if (!fallback) {
    throw new Error("No webhook fixtures are configured.");
  }

  return webhookFixtures.find((fixture) => fixture.kind === kind) ?? fallback;
}

export async function fireGithubWebhook(workspaceId: string, fixture: WebhookFixture, payload: JsonValue): Promise<IngestResult> {
  const headers = requestHeaders();
  headers.set("x-github-event", fixture.githubEvent);
  headers.set("x-github-delivery", crypto.randomUUID());
  headers.set("x-workspace-id", workspaceId);

  const response = await fetch("/v1/ingest/github", {
    method: "POST",
    headers,
    body: JSON.stringify(payload),
  });
  const data = await parseResponse(response);

  if (!response.ok) {
    return {
      ok: false,
      status: response.status,
      data: data as JsonValue | null,
      detail: extractDetail(data, response.statusText),
    };
  }

  return {
    ok: true,
    status: response.status,
    data: data as IngestResult["data"],
  };
}
