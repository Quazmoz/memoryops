import { extractDetail, parseResponse, requestHeaders } from "./client";
import type { IngestResult, JsonValue } from "./types";

export type WebhookSource = "github" | "slack" | "linear" | "jira";

export type GitHubWebhookKind =
  | "pull_request_opened"
  | "pull_request_merged"
  | "push"
  | "pull_request_review_approved"
  | "issue"
  | "issue_comment";

export type WebhookFixtureKind =
  | GitHubWebhookKind
  | "slack_message"
  | "slack_reaction"
  | "slack_app_mention"
  | "linear_issue_created"
  | "linear_issue_updated"
  | "linear_comment_created"
  | "jira_issue_created"
  | "jira_issue_updated"
  | "jira_comment_created";

type GitHubEventHeader = "pull_request" | "push" | "pull_request_review" | "issues" | "issue_comment";

export type WebhookFixture = {
  kind: WebhookFixtureKind;
  source: WebhookSource;
  label: string;
  githubEvent?: GitHubEventHeader;
  actor: string;
  payload: JsonValue;
};

export const webhookSources: Array<{ source: WebhookSource; label: string }> = [
  { source: "github", label: "GitHub" },
  { source: "slack", label: "Slack" },
  { source: "linear", label: "Linear" },
  { source: "jira", label: "Jira" },
];

const now = "2026-04-27T15:20:30Z";
const unixNow = 1777303230;
const unixNowMs = 1777303230000;
const slackMessageTs = "1777303230.000200";
const slackThreadTs = "1777303200.000100";

export const webhookFixtures: WebhookFixture[] = [
  {
    kind: "pull_request_opened",
    source: "github",
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
    source: "github",
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
    source: "github",
    label: "push",
    githubEvent: "push",
    actor: "nora",
    payload: {
      ref: "refs/heads/main",
      before: "9fceb02f9fceb02f9fceb02f9fceb02f9fceb02f",
      after: "b6fc4c620b67d95f953a5c1c1230aaab5db5a1b0",
      pusher: { name: "nora", email: "nora@example.com" },
      repository: { full_name: "Quazmoz/memoryops", pushed_at: unixNow },
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
    source: "github",
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
    source: "github",
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
    source: "github",
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
  {
    kind: "slack_message",
    source: "slack",
    label: "message",
    actor: "U024BE7LH",
    payload: {
      token: "verification-token",
      team_id: "T012AB3C4",
      api_app_id: "A012AB3C4",
      type: "event_callback",
      event_id: "Ev0123456789",
      event_time: unixNow,
      event_context: "4-eyJldCI6Im1lc3NhZ2UiLCJ0aWQiOiJUMDEyQUIzQzQifQ",
      authorizations: [
        {
          enterprise_id: null,
          team_id: "T012AB3C4",
          user_id: "UAPPBOT01",
          is_bot: true,
          is_enterprise_install: false,
        },
      ],
      event: {
        type: "message",
        channel: "C05MEMOPS",
        user: "U024BE7LH",
        text: "Ship the retrieval trace filter after the workspace scope migration lands.",
        ts: slackMessageTs,
        thread_ts: slackThreadTs,
        channel_type: "channel",
      },
    },
  },
  {
    kind: "slack_reaction",
    source: "slack",
    label: "reaction_added",
    actor: "U07REACT9",
    payload: {
      token: "verification-token",
      team_id: "T012AB3C4",
      api_app_id: "A012AB3C4",
      type: "event_callback",
      event_id: "Ev9876543210",
      event_time: unixNow + 45,
      event_context: "4-eyJldCI6InJlYWN0aW9uX2FkZGVkIiwidGlkIjoiVDAxMkFCM0M0In0",
      authorizations: [
        {
          enterprise_id: null,
          team_id: "T012AB3C4",
          user_id: "UAPPBOT01",
          is_bot: true,
          is_enterprise_install: false,
        },
      ],
      event: {
        type: "reaction_added",
        user: "U07REACT9",
        reaction: "eyes",
        item_user: "U024BE7LH",
        item: {
          type: "message",
          channel: "C05MEMOPS",
          ts: slackMessageTs,
        },
        event_ts: "1777303275.000300",
      },
    },
  },
  {
    kind: "slack_app_mention",
    source: "slack",
    label: "app_mention",
    actor: "U06MENTION",
    payload: {
      token: "verification-token",
      team_id: "T012AB3C4",
      api_app_id: "A012AB3C4",
      type: "event_callback",
      event_id: "Ev1357924680",
      event_time: unixNow + 90,
      event_context: "4-eyJldCI6ImFwcF9tZW50aW9uIiwidGlkIjoiVDAxMkFCM0M0In0",
      authorizations: [
        {
          enterprise_id: null,
          team_id: "T012AB3C4",
          user_id: "UAPPBOT01",
          is_bot: true,
          is_enterprise_install: false,
        },
      ],
      event: {
        type: "app_mention",
        channel: "C05MEMOPS",
        user: "U06MENTION",
        text: "<@UAPPBOT01> remember that jira OPS-219 is blocked on the Linear migration task.",
        ts: "1777303320.000400",
        channel_type: "channel",
      },
    },
  },
  {
    kind: "linear_issue_created",
    source: "linear",
    label: "Issue created",
    actor: "Ada Lovelace",
    payload: {
      type: "Issue",
      action: "create",
      organizationId: "8f1d2a41-6bb7-4c04-89d8-8dd6cbb7c938",
      webhookId: "2f6d6b0f-a053-41ea-9e78-e9f2ef23e4d9",
      webhookTimestamp: now,
      createdAt: now,
      actor: {
        id: "9b346f2b-5a68-4f1a-9a4e-2b5e7f4b6d7a",
        name: "Ada Lovelace",
        email: "ada@example.com",
        url: "https://linear.app/memoryops/profiles/ada",
      },
      data: {
        id: "d4e2d9af-7f4c-4d60-8d8e-9d66b32a5c52",
        identifier: "OPS-412",
        number: 412,
        title: "Expose DLQ retry controls in integrations",
        description: "Operators need a way to retry or discard failed processor jobs without opening Redis.",
        priority: 2,
        priorityLabel: "High",
        url: "https://linear.app/memoryops/issue/OPS-412/expose-dlq-retry-controls-in-integrations",
        createdAt: now,
        updatedAt: now,
        state: { id: "state-todo", name: "Todo", type: "unstarted", color: "#bec2c8" },
        assignee: { id: "user-grace", name: "Grace Hopper", email: "grace@example.com" },
        team: { id: "team-ops", key: "OPS", name: "Operations" },
        project: { id: "project-control-center", name: "Control Center" },
        cycle: { id: "cycle-15", name: "M15/M16" },
      },
    },
  },
  {
    kind: "linear_issue_updated",
    source: "linear",
    label: "Issue updated",
    actor: "Grace Hopper",
    payload: {
      type: "Issue",
      action: "update",
      organizationId: "8f1d2a41-6bb7-4c04-89d8-8dd6cbb7c938",
      webhookId: "2f6d6b0f-a053-41ea-9e78-e9f2ef23e4d9",
      webhookTimestamp: now,
      createdAt: now,
      actor: {
        id: "user-grace",
        name: "Grace Hopper",
        email: "grace@example.com",
        url: "https://linear.app/memoryops/profiles/grace",
      },
      data: {
        id: "d4e2d9af-7f4c-4d60-8d8e-9d66b32a5c52",
        identifier: "OPS-412",
        number: 412,
        title: "Expose DLQ retry controls in integrations",
        description: "Retry/discard buttons now need optimistic cache updates and inline errors.",
        priority: 2,
        priorityLabel: "High",
        url: "https://linear.app/memoryops/issue/OPS-412/expose-dlq-retry-controls-in-integrations",
        createdAt: "2026-04-26T12:00:00Z",
        updatedAt: now,
        state: { id: "state-progress", name: "In Progress", type: "started", color: "#f2c94c" },
        assignee: { id: "user-grace", name: "Grace Hopper", email: "grace@example.com" },
        team: { id: "team-ops", key: "OPS", name: "Operations" },
        project: { id: "project-control-center", name: "Control Center" },
        cycle: { id: "cycle-15", name: "M15/M16" },
      },
      updatedFrom: {
        stateId: "state-todo",
        updatedAt: "2026-04-26T12:00:00Z",
      },
    },
  },
  {
    kind: "linear_comment_created",
    source: "linear",
    label: "Comment created",
    actor: "Lin Chen",
    payload: {
      type: "Comment",
      action: "create",
      organizationId: "8f1d2a41-6bb7-4c04-89d8-8dd6cbb7c938",
      webhookId: "7f6b1f24-54cb-4d83-9983-b83d7ac74741",
      webhookTimestamp: now,
      createdAt: now,
      actor: { id: "user-lin", name: "Lin Chen", email: "lin@example.com" },
      data: {
        id: "comment-6a578ea0",
        body: "Retry should remove the failed job immediately, then restore it if the API rejects the action.",
        url: "https://linear.app/memoryops/issue/OPS-412#comment-6a578ea0",
        createdAt: now,
        updatedAt: now,
        user: { id: "user-lin", name: "Lin Chen", email: "lin@example.com" },
        issue: {
          id: "d4e2d9af-7f4c-4d60-8d8e-9d66b32a5c52",
          identifier: "OPS-412",
          title: "Expose DLQ retry controls in integrations",
          url: "https://linear.app/memoryops/issue/OPS-412/expose-dlq-retry-controls-in-integrations",
        },
      },
    },
  },
  {
    kind: "jira_issue_created",
    source: "jira",
    label: "jira:issue_created",
    actor: "Maya Patel",
    payload: {
      timestamp: unixNowMs,
      webhookEvent: "jira:issue_created",
      issue_event_type_name: "issue_created",
      user: {
        accountId: "712020:6e8a3b84-2dc5-44e4-b097-87fe9f6eb111",
        displayName: "Maya Patel",
        emailAddress: "maya@example.com",
        active: true,
      },
      issue: {
        id: "10041",
        self: "https://memoryops.atlassian.net/rest/api/3/issue/10041",
        key: "OPS-219",
        fields: {
          summary: "Document webhook fixture coverage",
          description: "Add realistic Slack, Linear, and Jira examples to the developer tester.",
          status: { id: "10000", name: "To Do", statusCategory: { key: "new", name: "To Do" } },
          priority: { id: "2", name: "High" },
          assignee: { accountId: "712020:assignee", displayName: "Noor Ahmed", emailAddress: "noor@example.com" },
          project: { id: "10010", key: "OPS", name: "Operations" },
          issuetype: { id: "10001", name: "Task" },
          created: now,
          updated: now,
        },
      },
    },
  },
  {
    kind: "jira_issue_updated",
    source: "jira",
    label: "jira:issue_updated",
    actor: "Noor Ahmed",
    payload: {
      timestamp: unixNowMs,
      webhookEvent: "jira:issue_updated",
      issue_event_type_name: "issue_updated",
      user: {
        accountId: "712020:assignee",
        displayName: "Noor Ahmed",
        emailAddress: "noor@example.com",
        active: true,
      },
      issue: {
        id: "10041",
        self: "https://memoryops.atlassian.net/rest/api/3/issue/10041",
        key: "OPS-219",
        fields: {
          summary: "Document webhook fixture coverage",
          description: "Add realistic Slack, Linear, and Jira examples to the developer tester.",
          status: { id: "3", name: "In Progress", statusCategory: { key: "indeterminate", name: "In Progress" } },
          priority: { id: "2", name: "High" },
          assignee: { accountId: "712020:assignee", displayName: "Noor Ahmed", emailAddress: "noor@example.com" },
          project: { id: "10010", key: "OPS", name: "Operations" },
          issuetype: { id: "10001", name: "Task" },
          created: "2026-04-26T12:00:00Z",
          updated: now,
        },
      },
      changelog: {
        id: "10088",
        items: [
          {
            field: "status",
            fieldtype: "jira",
            fieldId: "status",
            from: "10000",
            fromString: "To Do",
            to: "3",
            toString: "In Progress",
          },
        ],
      },
    },
  },
  {
    kind: "jira_comment_created",
    source: "jira",
    label: "comment_created",
    actor: "Iris Kim",
    payload: {
      timestamp: unixNowMs,
      webhookEvent: "comment_created",
      issue_event_type_name: "issue_commented",
      user: {
        accountId: "712020:commenter",
        displayName: "Iris Kim",
        emailAddress: "iris@example.com",
        active: true,
      },
      issue: {
        id: "10041",
        self: "https://memoryops.atlassian.net/rest/api/3/issue/10041",
        key: "OPS-219",
        fields: {
          summary: "Document webhook fixture coverage",
          status: { id: "3", name: "In Progress" },
          project: { id: "10010", key: "OPS", name: "Operations" },
        },
      },
      comment: {
        id: "10092",
        self: "https://memoryops.atlassian.net/rest/api/3/issue/10041/comment/10092",
        author: { accountId: "712020:commenter", displayName: "Iris Kim", emailAddress: "iris@example.com" },
        body: "Linked this Jira task back to Linear OPS-412 so the fixture work has one trail.",
        created: now,
        updated: now,
      },
    },
  },
];

export function fixturesForSource(source: WebhookSource): WebhookFixture[] {
  return webhookFixtures.filter((fixture) => fixture.source === source);
}

export function firstFixtureForSource(source: WebhookSource): WebhookFixture {
  const fixture = webhookFixtures.find((candidate) => candidate.source === source);
  if (!fixture) {
    throw new Error(`No webhook fixtures are configured for ${source}.`);
  }

  return fixture;
}

export function fixtureFor(kind: WebhookFixtureKind): WebhookFixture {
  const fallback = webhookFixtures[0];
  if (!fallback) {
    throw new Error("No webhook fixtures are configured.");
  }

  return webhookFixtures.find((fixture) => fixture.kind === kind) ?? fallback;
}

export async function fireWebhook(workspaceId: string, fixture: WebhookFixture, payload: JsonValue): Promise<IngestResult> {
  const headers = requestHeaders();
  headers.set("x-workspace-id", workspaceId);

  const endpoint = applySourceHeaders(headers, fixture);
  const response = await fetch(endpoint, {
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

function applySourceHeaders(headers: Headers, fixture: WebhookFixture): string {
  switch (fixture.source) {
    case "github": {
      if (!fixture.githubEvent) {
        throw new Error("GitHub fixture is missing an event header.");
      }

      headers.set("x-github-event", fixture.githubEvent);
      headers.set("x-github-delivery", crypto.randomUUID());
      return "/v1/ingest/github";
    }
    case "slack": {
      headers.set("x-slack-signature", `v0=${dummyHexSignature()}`);
      headers.set("x-slack-request-timestamp", Math.floor(Date.now() / 1000).toString());
      return "/v1/ingest/slack";
    }
    case "linear": {
      headers.set("x-linear-signature", dummyHexSignature());
      return "/v1/ingest/linear";
    }
    case "jira": {
      headers.set("x-hub-signature", `sha256=${dummyHexSignature()}`);
      headers.set("x-atlassian-webhook-identifier", crypto.randomUUID());
      return "/v1/ingest/jira";
    }
  }
}

function dummyHexSignature(): string {
  return "0".repeat(64);
}
