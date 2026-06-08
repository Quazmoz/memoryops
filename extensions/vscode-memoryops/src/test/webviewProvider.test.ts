import assert from "node:assert/strict";
import Module from "node:module";
import test from "node:test";

test("webview inline script parses successfully", async () => {
  const cjsModule = Module as unknown as {
    _load: (request: string, parent: unknown, isMain: boolean) => unknown;
  };
  const originalLoad = cjsModule._load;
  cjsModule._load = function patchedLoad(request: string, parent: unknown, isMain: boolean): unknown {
    if (request === "vscode") {
      return {};
    }
    return originalLoad.call(this, request, parent, isMain);
  };

  try {
    const { MemoryWebviewViewProvider } = await import("../webviewProvider.js");
    const provider = new MemoryWebviewViewProvider({} as never);
    const html = (provider as unknown as {
      _getHtmlForWebview: (webview: { cspSource: string }) => string;
    })._getHtmlForWebview({ cspSource: "vscode-webview://memoryops-test" });

    const scriptMatch = html.match(/<script[^>]*>([\s\S]*)<\/script>/);
    assert.ok(scriptMatch, "expected generated webview HTML to include a script block");

    assert.doesNotThrow(() => {
      // Parse only; do not execute. This catches broken escaping in the inline script.
      // eslint-disable-next-line no-new-func
      new Function(scriptMatch![1]);
    });
  } finally {
    cjsModule._load = originalLoad;
  }
});
