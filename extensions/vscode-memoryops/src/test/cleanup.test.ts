import assert from "node:assert/strict";
import test from "node:test";

import { removeMemoryOpsSettings } from "../cleanup";

test("removeMemoryOpsSettings strips memoryops keys from settings.json while preserving unrelated settings", () => {
  const input = `{
  // Keep this comment.
  "editor.fontSize": 14,
  "memoryops.apiUrl": "https://memoryops.test",
  "memoryops.workspaceId": "workspace-123",
  "files.autoSave": "off"
}
`;

  const result = removeMemoryOpsSettings(input);

  assert.equal(result.changed, true);
  assert.match(result.content, /editor\.fontSize/);
  assert.match(result.content, /files\.autoSave/);
  assert.doesNotMatch(result.content, /memoryops\.apiUrl/);
  assert.doesNotMatch(result.content, /memoryops\.workspaceId/);
  assert.match(result.content, /Keep this comment/);
});

test("removeMemoryOpsSettings strips nested workspace settings from .code-workspace files", () => {
  const input = `{
  "folders": [
    { "path": "." }
  ],
  "settings": {
    "memoryops.apiKey": "legacy-key",
    "editor.tabSize": 2
  }
}
`;

  const result = removeMemoryOpsSettings(input, ["settings"]);

  assert.equal(result.changed, true);
  assert.match(result.content, /editor\.tabSize/);
  assert.doesNotMatch(result.content, /memoryops\.apiKey/);
  assert.match(result.content, /"settings"/);
});

test("removeMemoryOpsSettings removes an empty settings object from .code-workspace files", () => {
  const input = `{
  "folders": [
    { "path": "." }
  ],
  "settings": {
    "memoryops.apiUrl": "https://memoryops.test"
  }
}
`;

  const result = removeMemoryOpsSettings(input, ["settings"]);

  assert.equal(result.changed, true);
  assert.doesNotMatch(result.content, /memoryops\.apiUrl/);
  assert.doesNotMatch(result.content, /"settings"/);
  assert.match(result.content, /"folders"/);
});
