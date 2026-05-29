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

function jsonResponse(value: unknown): Response {
  return new Response(JSON.stringify(value), {
    status: 200,
    headers: {
      "Content-Type": "application/json",
    },
  });
}