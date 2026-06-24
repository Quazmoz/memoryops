import * as vscode from "vscode";
import { MemoryOpsClient, MemorySearchResult } from "./client";
import { MemoryOpsConfig } from "./config";
import { getRelativeFileName } from "./repo";

const CACHE_TTL_MS = 60_000;
const SAMPLE_LIMIT = 10;

interface CodeLensClientContext {
  client: MemoryOpsClient;
  config: MemoryOpsConfig;
  missing: string[];
}

type GetClient = () => CodeLensClientContext;

interface FileMemoryCount {
  // Exact count of memories referencing the file (server-side `total`).
  total: number;
  // A bounded sample of those memories (for quick surfacing).
  items: MemorySearchResult[];
}

interface CacheEntry {
  expiresAt: number;
  value: FileMemoryCount;
}

/**
 * Shows an inline CodeLens at the top of source files indicating how many
 * MemoryOps memories reference the current file (e.g. "$(database) 3 memories
 * reference this file"). Clicking it surfaces those memories.
 *
 * Uses the backend's `source_ref` list filter, which matches memories by the
 * file recorded on their originating observation (line anchors ignored) — an
 * exact count, not a fuzzy search.
 *
 * Disabled by default — gated on `memoryops.enableCodeLens` — because it issues
 * a (cached) request per file. Results are cached per file for 60s.
 */
export class MemoryCodeLensProvider implements vscode.CodeLensProvider {
  private readonly _onDidChangeCodeLenses = new vscode.EventEmitter<void>();
  public readonly onDidChangeCodeLenses = this._onDidChangeCodeLenses.event;

  private readonly cache = new Map<string, CacheEntry>();
  private inFlight = new Map<string, Promise<FileMemoryCount>>();

  constructor(private readonly getClient: GetClient) {}

  /** Force CodeLenses to be recomputed (e.g. after config changes). */
  public refresh(): void {
    this.cache.clear();
    this.inFlight.clear();
    this._onDidChangeCodeLenses.fire();
  }

  public provideCodeLenses(
    document: vscode.TextDocument,
    token: vscode.CancellationToken,
  ): vscode.CodeLens[] {
    const { client, config, missing } = this.getClient();
    if (!config.enableCodeLens || missing.length > 0) {
      return [];
    }
    if (document.uri.scheme !== "file" || document.lineCount === 0) {
      return [];
    }

    const fileName = getRelativeFileName(document);
    if (!fileName) {
      return [];
    }

    const result = this.getCountForFile(client, fileName);
    if (token.isCancellationRequested || !result || result.total === 0) {
      return [];
    }

    const range = new vscode.Range(0, 0, 0, 0);
    const label = result.total === 1 ? "1 memory references this file" : `${result.total} memories reference this file`;
    return [
      new vscode.CodeLens(range, {
        title: `$(database) ${label}`,
        command: "memoryops.showMemoriesForFile",
        arguments: [fileName],
      }),
    ];
  }

  private getCountForFile(
    client: MemoryOpsClient,
    fileName: string,
  ): FileMemoryCount | undefined {
    const cached = this.cache.get(fileName);
    if (cached && Date.now() < cached.expiresAt) {
      return cached.value;
    }

    const existing = this.inFlight.get(fileName);
    if (existing) {
      return undefined;
    }

    const promise = (async () => {
      const response = await client.listMemory({ sourceRef: fileName, limit: SAMPLE_LIMIT });
      const value: FileMemoryCount = { total: response.total, items: response.items };
      this.cache.set(fileName, { expiresAt: Date.now() + CACHE_TTL_MS, value });
      this.inFlight.delete(fileName);
      // Trigger a re-render now that the real count is known.
      this._onDidChangeCodeLenses.fire();
      return value;
    })().catch((error) => {
      this.inFlight.delete(fileName);
    });

    this.inFlight.set(fileName, promise as any);
    return undefined;
  }

  public dispose(): void {
    this._onDidChangeCodeLenses.dispose();
    this.cache.clear();
    this.inFlight.clear();
  }
}
