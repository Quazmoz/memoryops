import * as vscode from "vscode";
import { MemoryOpsClient, MemorySearchResult } from "./client";
import { MemoryOpsConfig } from "./config";
import { getRelativeFileName, getWorkspaceRepoHint } from "./repo";

const CACHE_TTL_MS = 60_000;
const MAX_LENS_TOP_K = 10;

interface CodeLensClientContext {
  client: MemoryOpsClient;
  config: MemoryOpsConfig;
  missing: string[];
}

type GetClient = () => CodeLensClientContext;

interface CacheEntry {
  expiresAt: number;
  results: MemorySearchResult[];
}

/**
 * Shows an inline CodeLens at the top of source files indicating how many
 * MemoryOps memories reference the current file (e.g. "$(database) 3 memories
 * reference this file"). Clicking it surfaces those memories.
 *
 * Disabled by default — gated on `memoryops.enableCodeLens` — because it issues
 * a (cached) search request per file. Results are cached per file for 60s.
 */
export class MemoryCodeLensProvider implements vscode.CodeLensProvider {
  private readonly _onDidChangeCodeLenses = new vscode.EventEmitter<void>();
  public readonly onDidChangeCodeLenses = this._onDidChangeCodeLenses.event;

  private readonly cache = new Map<string, CacheEntry>();
  private inFlight = new Map<string, Promise<MemorySearchResult[]>>();

  constructor(private readonly getClient: GetClient) {}

  /** Force CodeLenses to be recomputed (e.g. after config changes). */
  public refresh(): void {
    this.cache.clear();
    this.inFlight.clear();
    this._onDidChangeCodeLenses.fire();
  }

  public async provideCodeLenses(
    document: vscode.TextDocument,
    token: vscode.CancellationToken,
  ): Promise<vscode.CodeLens[]> {
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

    let results: MemorySearchResult[];
    try {
      results = await this.getResultsForFile(client, config, document, fileName);
    } catch {
      // Never surface CodeLens errors inline — fail silent.
      return [];
    }

    if (token.isCancellationRequested || results.length === 0) {
      return [];
    }

    const range = new vscode.Range(0, 0, 0, 0);
    const label = results.length === 1 ? "1 memory references this file" : `${results.length} memories reference this file`;
    return [
      new vscode.CodeLens(range, {
        title: `$(database) ${label}`,
        command: "memoryops.showMemoriesForFile",
        arguments: [fileName],
      }),
    ];
  }

  private async getResultsForFile(
    client: MemoryOpsClient,
    config: MemoryOpsConfig,
    document: vscode.TextDocument,
    fileName: string,
  ): Promise<MemorySearchResult[]> {
    const cached = this.cache.get(fileName);
    if (cached && Date.now() < cached.expiresAt) {
      return cached.results;
    }

    const existing = this.inFlight.get(fileName);
    if (existing) {
      return existing;
    }

    const promise = (async () => {
      const repo = await getWorkspaceRepoHint(document);
      const results = await client.searchMemory(fileName, MAX_LENS_TOP_K, {
        mode: config.defaultSearchMode,
        repo,
        includeWorkspacePool: config.includeWorkspacePool,
      });
      this.cache.set(fileName, { expiresAt: Date.now() + CACHE_TTL_MS, results });
      this.inFlight.delete(fileName);
      // Trigger a re-render now that the real count is known.
      this._onDidChangeCodeLenses.fire();
      return results;
    })().catch((error) => {
      this.inFlight.delete(fileName);
      throw error;
    });

    this.inFlight.set(fileName, promise);
    return promise;
  }

  public dispose(): void {
    this._onDidChangeCodeLenses.dispose();
    this.cache.clear();
    this.inFlight.clear();
  }
}
