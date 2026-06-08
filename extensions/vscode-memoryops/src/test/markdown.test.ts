import assert from "node:assert/strict";
import test from "node:test";

import {
  firstLine,
  formatMemoryFeedbackMarkdown,
  formatMemoryHistoryMarkdown,
  formatMemoryProvenanceMarkdown,
  formatRetrievalMarkdown,
} from "../markdown";

test("firstLine extracts first non-empty line and handles edge cases", () => {
  assert.equal(firstLine("Hello\nWorld"), "Hello");
  assert.equal(firstLine("\n\nFirst non-empty line\nSecond line"), "First non-empty line");
  assert.equal(firstLine("   \n\n  Spaced non-empty line  \nOther"), "  Spaced non-empty line  ");
  assert.equal(firstLine("\n\n\n"), "");
  assert.equal(firstLine(""), "");
});

test("formatMemoryHistoryMarkdown renders version metadata and body", () => {
  const markdown = formatMemoryHistoryMarkdown(
    { id: "mem-1" },
    [
      {
        version: 3,
        edited_by: "vscode",
        created_at: "2026-05-29T12:00:00Z",
        importance_score: 0.82,
        tags: ["decision", "release"],
        content: "Updated rollout checklist",
      },
    ],
  );

  assert.match(markdown, /MemoryOps Memory History/);
  assert.match(markdown, /Version 3/);
  assert.match(markdown, /Edited by: vscode/);
  assert.match(markdown, /Updated rollout checklist/);
});

test("formatMemoryProvenanceMarkdown renders nodes and edges", () => {
  const markdown = formatMemoryProvenanceMarkdown(
    { id: "mem-1" },
    {
      root_id: "mem-1",
      nodes: [
        {
          id: "mem-1",
          node_type: "memory",
          title: "Deployment note",
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
    },
  );

  assert.match(markdown, /Nodes: 1/);
  assert.match(markdown, /Deployment note/);
  assert.match(markdown, /derived_from/);
  assert.match(markdown, /"source": "vscode"/);
});

test("formatMemoryFeedbackMarkdown handles empty feedback", () => {
  const markdown = formatMemoryFeedbackMarkdown(
    { id: "mem-1" },
    {
      items: [],
      total: 0,
    },
  );

  assert.match(markdown, /Entries: 0/);
  assert.match(markdown, /No retrieval feedback has been recorded/);
});

test("formatRetrievalMarkdown includes query id and memory snippets", () => {
  const markdown = formatRetrievalMarkdown(
    {
      query_id: "query-1",
      total_tokens: 512,
      packed_context: "Relevant context block",
      memories: [
        {
          id: "mem-1",
          score: 0.93,
          memory_type: "semantic",
          content: "Deploy after smoke tests finish.",
        },
      ],
    },
    "MemoryOps Context",
  );

  assert.match(markdown, /Query ID: `query-1`/);
  assert.match(markdown, /Total tokens: 512/);
  assert.match(markdown, /Deploy after smoke tests finish/);
});