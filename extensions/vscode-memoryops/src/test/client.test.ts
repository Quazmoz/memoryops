import assert from "node:assert/strict";
import test from "node:test";

import { MemoryOpsClient } from "../client";

const CONFIG = {
  apiUrl: "https://memoryops.test",
  workspaceId: "workspace-123",
  apiKey: "secret-key",
  defaultTopK: 5,
  defaultSearchMode: "hybrid" as const,
  defaultTokenBudget: 2048,
  sidebarPageSize: 20,
  includeWorkspacePool: false,
  defaultAgentId: "vscode",
  maxRetries: 0,
  retryBackoffMs: 0,
  enableCodeLens: false,
};

test("promoteMemory issues a workspace-scoped POST and normalizes the response", async () => {
  let receivedUrl = "";
  let receivedMethod = "";
  const restoreFetch = mockFetch(async (url, init) => {
    receivedUrl = url;
    receivedMethod = init?.method ?? "GET";
    return jsonResponse({
      id: "mem-1",
      memory_type: "semantic",
      scope_visibility: "workspace",
      pinned: true,
    });
  });

  try {
    const client = new MemoryOpsClient(CONFIG);
    const result = await client.promoteMemory("mem-1");

    assert.equal(receivedMethod, "POST");
    assert.equal(receivedUrl, "https://memoryops.test/v1/memory/mem-1/promote?workspace_id=workspace-123");
    assert.equal(result.id, "mem-1");
    assert.equal(result.memory_type, "semantic");
    assert.equal(result.scope_visibility, "workspace");
    assert.equal(result.pinned, true);
  } finally {
    restoreFetch();
  }
});

test("getMemoryHistory normalizes version rows", async () => {
  const restoreFetch = mockFetch(async () => jsonResponse({
    items: [
      {
        id: "version-1",
        memory_id: "mem-1",
        workspace_id: "workspace-123",
        version: 3,
        content: "Updated memory body",
        importance_score: 0.82,
        tags: ["decision", "release"],
        edited_by: "vscode",
        created_at: "2026-05-29T12:00:00Z",
      },
    ],
  }));

  try {
    const client = new MemoryOpsClient(CONFIG);
    const versions = await client.getMemoryHistory("mem-1");

    assert.equal(versions.length, 1);
    assert.deepEqual(versions[0], {
      id: "version-1",
      memory_id: "mem-1",
      workspace_id: "workspace-123",
      version: 3,
      content: "Updated memory body",
      importance_score: 0.82,
      tags: ["decision", "release"],
      edited_by: "vscode",
      created_at: "2026-05-29T12:00:00Z",
    });
  } finally {
    restoreFetch();
  }
});

test("getMemoryProvenance normalizes nodes and edges", async () => {
  let receivedUrl = "";
  const restoreFetch = mockFetch(async (url) => {
    receivedUrl = url;
    return jsonResponse({
      root_id: "mem-1",
      nodes: [
        {
          id: "mem-1",
          node_type: "memory",
          title: "Deployment note",
          subtitle: "semantic memory",
          timestamp: "2026-05-29T12:00:00Z",
          metadata: {
            source: "vscode",
          },
        },
      ],
      edges: [
        {
          from: "event-1",
          to: "mem-1",
          edge_type: "derived_from",
        },
      ],
    });
  });

  try {
    const client = new MemoryOpsClient(CONFIG);
    const graph = await client.getMemoryProvenance("mem-1");

    assert.equal(receivedUrl, "https://memoryops.test/v1/memory/mem-1/provenance?workspace_id=workspace-123");
    assert.equal(graph.root_id, "mem-1");
    assert.equal(graph.nodes.length, 1);
    assert.equal(graph.nodes[0]?.metadata?.source, "vscode");
    assert.equal(graph.edges[0]?.edge_type, "derived_from");
  } finally {
    restoreFetch();
  }
});

test("getMemoryFeedback includes list query params and normalizes ratings", async () => {
  let receivedUrl = "";
  const restoreFetch = mockFetch(async (url) => {
    receivedUrl = url;
    return jsonResponse({
      items: [
        {
          id: "feedback-1",
          memory_id: "mem-1",
          query_id: "query-1",
          agent_id: "vscode",
          user_id: "dev",
          rating: 1,
          comment: "Useful context",
          occurred_at: "2026-05-29T13:00:00Z",
        },
      ],
      total: 1,
      memory_id: "mem-1",
      avg_rating: 1,
      relevance_score: 0.91,
    });
  });

  try {
    const client = new MemoryOpsClient(CONFIG);
    const feedback = await client.getMemoryFeedback("mem-1", { limit: 25, offset: 50 });

    assert.equal(
      receivedUrl,
      "https://memoryops.test/v1/memory/mem-1/feedback?workspace_id=workspace-123&limit=25&offset=50",
    );
    assert.equal(feedback.total, 1);
    assert.equal(feedback.memory_id, "mem-1");
    assert.equal(feedback.avg_rating, 1);
    assert.equal(feedback.items[0]?.rating, 1);
    assert.equal(feedback.items[0]?.comment, "Useful context");
  } finally {
    restoreFetch();
  }
});

test("submitMemoryFeedback sends a POST with feedback fields", async () => {
  let receivedUrl = "";
  let receivedMethod = "";
  let receivedBody: any = null;

  const restoreFetch = mockFetch(async (url, init) => {
    receivedUrl = url;
    receivedMethod = init?.method ?? "";
    receivedBody = init?.body ? JSON.parse(init.body as string) : null;
    return jsonResponse({ success: true });
  });

  try {
    const client = new MemoryOpsClient(CONFIG);
    await client.submitMemoryFeedback("mem-1", {
      queryId: "query-abc",
      rating: 1,
      comment: "Very helpful",
      agentId: "vscode",
    });

    assert.equal(receivedUrl, "https://memoryops.test/v1/memory/mem-1/feedback?workspace_id=workspace-123");
    assert.equal(receivedMethod, "POST");
    assert.deepEqual(receivedBody, {
      query_id: "query-abc",
      rating: 1,
      comment: "Very helpful",
      agent_id: "vscode",
    });
  } finally {
    restoreFetch();
  }
});

test("searchMemory retries transient 503s on idempotent reads and eventually succeeds", async () => {
  let attempts = 0;
  const restoreFetch = mockFetch(async () => {
    attempts++;
    if (attempts < 3) {
      return jsonResponse({ detail: "temporarily unavailable" }, 503);
    }
    return jsonResponse({ results: [{ id: "mem-9", content: "hit", score: 0.9 }] });
  });

  try {
    const client = new MemoryOpsClient({ ...CONFIG, maxRetries: 3, retryBackoffMs: 1 });
    const results = await client.searchMemory("query", 5);

    assert.equal(attempts, 3);
    assert.equal(results.length, 1);
    assert.equal(results[0].id, "mem-9");
  } finally {
    restoreFetch();
  }
});

test("retries are exhausted and the last error is surfaced", async () => {
  let attempts = 0;
  const restoreFetch = mockFetch(async () => {
    attempts++;
    return jsonResponse({ detail: "still down" }, 503);
  });

  try {
    const client = new MemoryOpsClient({ ...CONFIG, maxRetries: 2, retryBackoffMs: 1 });
    await assert.rejects(() => client.searchMemory("query", 5), /MemoryOps 503/);
    assert.equal(attempts, 3); // initial attempt + 2 retries
  } finally {
    restoreFetch();
  }
});

test("mutating writes are never retried even on transient failures", async () => {
  let attempts = 0;
  const restoreFetch = mockFetch(async () => {
    attempts++;
    return jsonResponse({ detail: "down" }, 503);
  });

  try {
    const client = new MemoryOpsClient({ ...CONFIG, maxRetries: 5, retryBackoffMs: 1 });
    await assert.rejects(() => client.updateMemory("mem-1", { pinned: true }), /MemoryOps 503/);
    assert.equal(attempts, 1); // no retry for PATCH
  } finally {
    restoreFetch();
  }
});

test("listSkills uses the workspace route and keeps published visibility", async () => {
  let receivedUrl = "";
  const restoreFetch = mockFetch(async (url) => {
    receivedUrl = url;
    return jsonResponse([
      {
        id: "skill-1",
        workspace_id: "workspace-123",
        name: "release_notes",
        description: "Generate release notes",
        endpoint_url: "https://skills.test/release-notes",
        http_method: "POST",
        input_schema: { type: "object" },
        output_schema: { type: "object" },
        auth_header: null,
        enabled: true,
        version: 4,
        scope_visibility: "published",
      },
    ]);
  });

  try {
    const client = new MemoryOpsClient(CONFIG);
    const skills = await client.listSkills();

    assert.equal(receivedUrl, "https://memoryops.test/v1/workspaces/workspace-123/skills");
    assert.equal(skills.length, 1);
    assert.equal(skills[0]?.name, "release_notes");
    assert.equal(skills[0]?.scope_visibility, "published");
    assert.equal(skills[0]?.version, 4);
  } finally {
    restoreFetch();
  }
});

test("createSkill sends configured fields including scope visibility", async () => {
  let receivedUrl = "";
  let receivedMethod = "";
  let receivedBody: Record<string, unknown> | null = null;

  const restoreFetch = mockFetch(async (url, init) => {
    receivedUrl = url;
    receivedMethod = init?.method ?? "GET";
    receivedBody = init?.body ? JSON.parse(init.body as string) : null;
    return jsonResponse({
      id: "skill-2",
      workspace_id: "workspace-123",
      name: "slack_post",
      description: "Post to Slack",
      endpoint_url: "https://skills.test/slack",
      http_method: "POST",
      input_schema: { type: "object" },
      output_schema: { type: "object" },
      auth_header: "Authorization",
      enabled: true,
      version: 1,
      scope_visibility: "workspace",
    });
  });

  try {
    const client = new MemoryOpsClient(CONFIG);
    const skill = await client.createSkill({
      name: "slack_post",
      description: "Post to Slack",
      endpoint_url: "https://skills.test/slack",
      auth_header: "Authorization",
      auth_secret: "Bearer token",
      change_note: "initial setup",
      scope_visibility: "workspace",
    });

    assert.equal(receivedMethod, "POST");
    assert.equal(receivedUrl, "https://memoryops.test/v1/workspaces/workspace-123/skills");
    assert.deepEqual(receivedBody, {
      name: "slack_post",
      description: "Post to Slack",
      endpoint_url: "https://skills.test/slack",
      http_method: "POST",
      input_schema: {},
      output_schema: {},
      auth_header: "Authorization",
      auth_secret: "Bearer token",
      enabled: true,
      change_note: "initial setup",
      scope_visibility: "workspace",
    });
    assert.equal(skill.scope_visibility, "workspace");
  } finally {
    restoreFetch();
  }
});

test("updateSkill sends partial patches for skill toggles", async () => {
  let receivedUrl = "";
  let receivedMethod = "";
  let receivedBody: Record<string, unknown> | null = null;

  const restoreFetch = mockFetch(async (url, init) => {
    receivedUrl = url;
    receivedMethod = init?.method ?? "GET";
    receivedBody = init?.body ? JSON.parse(init.body as string) : null;
    return jsonResponse({
      id: "skill-2",
      workspace_id: "workspace-123",
      name: "slack_post",
      description: "Post to Slack",
      endpoint_url: "https://skills.test/slack",
      http_method: "POST",
      input_schema: { type: "object" },
      output_schema: { type: "object" },
      auth_header: "Authorization",
      enabled: false,
      version: 2,
      scope_visibility: "published",
    });
  });

  try {
    const client = new MemoryOpsClient(CONFIG);
    const skill = await client.updateSkill("slack_post", {
      enabled: false,
      change_note: "disabled during incident",
      scope_visibility: "published",
    });

    assert.equal(receivedMethod, "PATCH");
    assert.equal(receivedUrl, "https://memoryops.test/v1/workspaces/workspace-123/skills/slack_post");
    assert.deepEqual(receivedBody, {
      enabled: false,
      change_note: "disabled during incident",
      scope_visibility: "published",
    });
    assert.equal(skill.enabled, false);
    assert.equal(skill.scope_visibility, "published");
  } finally {
    restoreFetch();
  }
});

test("testSkill wraps the request body and normalizes the result", async () => {
  let receivedUrl = "";
  let receivedMethod = "";
  let receivedBody: Record<string, unknown> | null = null;

  const restoreFetch = mockFetch(async (url, init) => {
    receivedUrl = url;
    receivedMethod = init?.method ?? "GET";
    receivedBody = init?.body ? JSON.parse(init.body as string) : null;
    return jsonResponse({
      status: 202,
      latency_ms: 143,
      body: { queued: true },
    });
  });

  try {
    const client = new MemoryOpsClient(CONFIG);
    const result = await client.testSkill("release_notes", { release: "1.0.0" });

    assert.equal(receivedMethod, "POST");
    assert.equal(receivedUrl, "https://memoryops.test/v1/workspaces/workspace-123/skills/release_notes/test");
    assert.deepEqual(receivedBody, { body: { release: "1.0.0" } });
    assert.deepEqual(result, {
      status: 202,
      latency_ms: 143,
      body: { queued: true },
    });
  } finally {
    restoreFetch();
  }
});

test("listSkillVersions and rollbackSkillVersion normalize historical skill data", async () => {
  let requests: Array<{ url: string; method: string; body: Record<string, unknown> | null }> = [];
  const restoreFetch = mockFetch(async (url, init) => {
    const method = init?.method ?? "GET";
    const body = init?.body ? JSON.parse(init.body as string) : null;
    requests.push({ url, method, body });

    if (url.endsWith("/versions")) {
      return jsonResponse([
        {
          id: "version-1",
          skill_id: "skill-2",
          workspace_id: "workspace-123",
          name: "slack_post",
          version: 1,
          description: "Post to Slack",
          endpoint_url: "https://skills.test/slack",
          http_method: "POST",
          input_schema: { type: "object" },
          output_schema: { type: "object" },
          auth_header: "Authorization",
          enabled: true,
          scope_visibility: "published",
          change_note: "initial setup",
          created_by: "vscode",
          created_at: "2026-06-01T12:00:00Z",
        },
      ]);
    }

    return jsonResponse({
      id: "skill-2",
      workspace_id: "workspace-123",
      name: "slack_post",
      description: "Post to Slack",
      endpoint_url: "https://skills.test/slack",
      http_method: "POST",
      input_schema: { type: "object" },
      output_schema: { type: "object" },
      auth_header: "Authorization",
      enabled: true,
      version: 3,
      scope_visibility: "published",
    });
  });

  try {
    const client = new MemoryOpsClient(CONFIG);
    const versions = await client.listSkillVersions("slack_post");
    const rolledBack = await client.rollbackSkillVersion("slack_post", 1, "restore known good config");

    assert.equal(requests[0]?.url, "https://memoryops.test/v1/workspaces/workspace-123/skills/slack_post/versions");
    assert.equal(requests[0]?.method, "GET");
    assert.equal(versions.length, 1);
    assert.equal(versions[0]?.scope_visibility, "published");
    assert.equal(versions[0]?.change_note, "initial setup");

    assert.equal(requests[1]?.url, "https://memoryops.test/v1/workspaces/workspace-123/skills/slack_post/versions/1/rollback");
    assert.equal(requests[1]?.method, "POST");
    assert.deepEqual(requests[1]?.body, { change_note: "restore known good config" });
    assert.equal(rolledBack.version, 3);
    assert.equal(rolledBack.scope_visibility, "published");
  } finally {
    restoreFetch();
  }
});

function mockFetch(handler: (url: string, init?: RequestInit) => Promise<Response> | Response): () => void {
  const originalFetch = globalThis.fetch;

  globalThis.fetch = (async (input: RequestInfo | URL, init?: RequestInit) => {
    const url = typeof input === "string"
      ? input
      : input instanceof URL
        ? input.toString()
        : input.url;

    return await handler(url, init);
  }) as typeof fetch;

  return () => {
    globalThis.fetch = originalFetch;
  };
}

function jsonResponse(value: unknown, status = 200): Response {
  return new Response(JSON.stringify(value), {
    status,
    headers: {
      "Content-Type": "application/json",
    },
  });
}