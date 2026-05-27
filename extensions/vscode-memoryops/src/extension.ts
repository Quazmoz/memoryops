import * as vscode from "vscode";

import { MemoryOpsClient, MemorySearchResult, RetrievalResult } from "./client";
import { getConfig, openMemoryOpsSettings, validateConfig } from "./config";
import { MemoryTreeProvider, memoryFromCommandArgument, memoryLabel } from "./memoryTree";
import { getRelativeFileName, getSourceRef, getWorkspaceRepoHint } from "./repo";

let statusBarItem: vscode.StatusBarItem;
let memoryTreeProvider: MemoryTreeProvider;

export function activate(context: vscode.ExtensionContext): void {
  memoryTreeProvider = new MemoryTreeProvider();
  context.subscriptions.push(vscode.window.registerTreeDataProvider("memoryops.memories", memoryTreeProvider));

  statusBarItem = vscode.window.createStatusBarItem(vscode.StatusBarAlignment.Left, 100);
  statusBarItem.command = "memoryops.testConnection";
  statusBarItem.text = "$(database) MemoryOps";
  statusBarItem.tooltip = "MemoryOps: Test connection";
  statusBarItem.show();
  context.subscriptions.push(statusBarItem);

  context.subscriptions.push(
    vscode.commands.registerCommand("memoryops.testConnection", testConnection),
    vscode.commands.registerCommand("memoryops.refreshMemories", refreshMemories),
    vscode.commands.registerCommand("memoryops.searchMemory", searchMemory),
    vscode.commands.registerCommand("memoryops.retrieveContextForCurrentFile", retrieveContextForCurrentFile),
    vscode.commands.registerCommand("memoryops.saveSelectionAsObservation", saveSelectionAsObservation),
    vscode.commands.registerCommand("memoryops.openMemory", openMemory),
    vscode.commands.registerCommand("memoryops.pinMemory", (item?: unknown) => setMemoryPinned(item, true)),
    vscode.commands.registerCommand("memoryops.unpinMemory", (item?: unknown) => setMemoryPinned(item, false)),
    vscode.commands.registerCommand("memoryops.deleteMemory", deleteMemory),
    vscode.commands.registerCommand("memoryops.copyMemory", copyMemory),
    vscode.commands.registerCommand("memoryops.openSettings", openMemoryOpsSettings),
  );
}

export function deactivate(): void {
  statusBarItem?.dispose();
}

async function testConnection(): Promise<void> {
  const { client, missing } = getClient();
  if (missing.length > 0) {
    await promptForMissingConfig(missing);
    return;
  }

  try {
    await vscode.window.withProgress(
      {
        location: vscode.ProgressLocation.Notification,
        title: "Testing MemoryOps connection...",
        cancellable: false,
      },
      async () => {
        await client.health();
        await client.getWorkspace();
      },
    );

    statusBarItem.text = "$(check) MemoryOps";
    statusBarItem.tooltip = "MemoryOps connected";
    void vscode.window.showInformationMessage("MemoryOps connection is healthy.");
  } catch (error) {
    statusBarItem.text = "$(error) MemoryOps";
    statusBarItem.tooltip = `MemoryOps connection failed: ${errorMessage(error)}`;
    throw error;
  }
}

async function refreshMemories(): Promise<void> {
  const { client, config, missing } = getClient();
  if (missing.length > 0) {
    await promptForMissingConfig(missing);
    return;
  }

  try {
    const response = await vscode.window.withProgress(
      {
        location: vscode.ProgressLocation.Window,
        title: "MemoryOps: refreshing memories...",
        cancellable: false,
      },
      () => client.listMemory({
        limit: config.sidebarPageSize,
        offset: 0,
        sort: "updated_at",
        direction: "desc",
      }),
    );

    memoryTreeProvider.setRecentMemories(response);
  } catch (error) {
    memoryTreeProvider.setError(errorMessage(error));
    throw error;
  }
}

async function searchMemory(): Promise<void> {
  const { client, config, missing } = getClient();
  if (missing.length > 0) {
    await promptForMissingConfig(missing);
    return;
  }

  const query = await vscode.window.showInputBox({
    title: "MemoryOps: Search Memory",
    prompt: "Enter a search query for your MemoryOps workspace.",
    ignoreFocusOut: true,
  });

  if (!query?.trim()) {
    return;
  }

  const repo = await getWorkspaceRepoHint(vscode.window.activeTextEditor?.document);
  const results = await vscode.window.withProgress(
    {
      location: vscode.ProgressLocation.Notification,
      title: "Searching MemoryOps...",
      cancellable: false,
    },
    () => client.searchMemory(query.trim(), config.defaultTopK, {
      mode: config.defaultSearchMode,
      repo,
      includeWorkspacePool: config.includeWorkspacePool,
    }),
  );

  memoryTreeProvider.setSearchResults(results, query.trim());
  await showSearchResults(results, `MemoryOps search: ${query.trim()}`);
}

async function retrieveContextForCurrentFile(): Promise<void> {
  const { client, config, missing } = getClient();
  if (missing.length > 0) {
    await promptForMissingConfig(missing);
    return;
  }

  const editor = vscode.window.activeTextEditor;
  const document = editor?.document;
  const selectedText = editor ? selectedTextOrEmpty(editor) : "";
  const repo = await getWorkspaceRepoHint(document);
  const fileName = document ? getRelativeFileName(document) : "current editor";

  const query = [
    `Relevant MemoryOps context for ${fileName}`,
    document?.languageId ? `Language: ${document.languageId}` : undefined,
    repo ? `Repository: ${repo}` : undefined,
    selectedText ? `Selected code/text:\n${truncate(selectedText, 4000)}` : undefined,
  ]
    .filter(Boolean)
    .join("\n\n");

  const result = await vscode.window.withProgress(
    {
      location: vscode.ProgressLocation.Notification,
      title: "Retrieving MemoryOps context...",
      cancellable: false,
    },
    () => client.retrieve(query, config.defaultTokenBudget, {
      mode: config.defaultSearchMode,
      repo,
      includeTrace: true,
      includeWorkspacePool: config.includeWorkspacePool,
    }),
  );

  if (Array.isArray(result.memories)) {
    memoryTreeProvider.setRetrievedMemories(result.memories, "No memories returned for current context.");
  }
  await showRetrievalResult(result, "MemoryOps Context");
}

async function saveSelectionAsObservation(): Promise<void> {
  const { client, config, missing } = getClient();
  if (missing.length > 0) {
    await promptForMissingConfig(missing);
    return;
  }

  const editor = vscode.window.activeTextEditor;
  if (!editor) {
    void vscode.window.showWarningMessage("Open a file and select text to save as a MemoryOps observation.");
    return;
  }

  const selectedText = selectedTextOrEmpty(editor);
  if (!selectedText.trim()) {
    void vscode.window.showWarningMessage("Select code, notes, or a decision before saving an observation.");
    return;
  }

  const tagsInput = await vscode.window.showInputBox({
    title: "MemoryOps: Save Selection as Observation",
    prompt: "Optional comma-separated tags.",
    value: "vscode,selection",
    ignoreFocusOut: true,
  });

  if (tagsInput === undefined) {
    return;
  }

  const tags = tagsInput
    .split(",")
    .map((tag) => tag.trim())
    .filter((tag) => tag.length > 0);

  const repo = await getWorkspaceRepoHint(editor.document);
  const sourceRef = getSourceRef(editor);

  const accepted = await vscode.window.withProgress(
    {
      location: vscode.ProgressLocation.Notification,
      title: "Saving MemoryOps observation...",
      cancellable: false,
    },
    () => client.saveObservation({
      content: selectedText.trim(),
      agentId: config.defaultAgentId,
      repo,
      tags,
      sourceRef,
    }),
  );

  void vscode.window.showInformationMessage(`MemoryOps observation queued: ${accepted.id}`);
  void refreshMemories();
}

async function openMemory(item?: unknown): Promise<void> {
  const memory = memoryFromCommandArgument(item);
  if (!memory) {
    void vscode.window.showWarningMessage("Select a MemoryOps memory first.");
    return;
  }

  await openMarkdownDocument(formatMemoryResult(memory));
}

async function setMemoryPinned(item: unknown, pinned: boolean): Promise<void> {
  const memory = memoryFromCommandArgument(item);
  if (!memory?.id) {
    void vscode.window.showWarningMessage("Select a MemoryOps memory first.");
    return;
  }

  const { client, missing } = getClient();
  if (missing.length > 0) {
    await promptForMissingConfig(missing);
    return;
  }

  const memoryId = memory.id;
  const updated = await vscode.window.withProgress(
    {
      location: vscode.ProgressLocation.Window,
      title: pinned ? "MemoryOps: pinning memory..." : "MemoryOps: unpinning memory...",
      cancellable: false,
    },
    () => client.updateMemory(memoryId, { pinned }),
  );

  memoryTreeProvider.updateMemory(updated);
  void vscode.window.showInformationMessage(`MemoryOps memory ${pinned ? "pinned" : "unpinned"}.`);
}

async function deleteMemory(item?: unknown): Promise<void> {
  const memory = memoryFromCommandArgument(item);
  if (!memory?.id) {
    void vscode.window.showWarningMessage("Select a MemoryOps memory first.");
    return;
  }

  const confirmed = await vscode.window.showWarningMessage(
    `Delete MemoryOps memory ${memoryLabel(memory)}?`,
    { modal: true },
    "Delete",
  );
  if (confirmed !== "Delete") {
    return;
  }

  const { client, missing } = getClient();
  if (missing.length > 0) {
    await promptForMissingConfig(missing);
    return;
  }

  const memoryId = memory.id;
  await vscode.window.withProgress(
    {
      location: vscode.ProgressLocation.Window,
      title: "MemoryOps: deleting memory...",
      cancellable: false,
    },
    () => client.deleteMemory(memoryId),
  );

  memoryTreeProvider.removeMemory(memoryId);
  void vscode.window.showInformationMessage("MemoryOps memory deleted.");
}

async function copyMemory(item?: unknown): Promise<void> {
  const memory = memoryFromCommandArgument(item);
  if (!memory?.content) {
    void vscode.window.showWarningMessage("Selected MemoryOps memory has no content to copy.");
    return;
  }

  await vscode.env.clipboard.writeText(memory.content);
  void vscode.window.showInformationMessage("MemoryOps memory content copied.");
}

function getClient(): { client: MemoryOpsClient; config: ReturnType<typeof getConfig>; missing: string[] } {
  const config = getConfig();
  return {
    config,
    client: new MemoryOpsClient(config),
    missing: validateConfig(config),
  };
}

async function promptForMissingConfig(missing: string[]): Promise<void> {
  const action = await vscode.window.showWarningMessage(
    `MemoryOps settings are incomplete: ${missing.join(", ")}`,
    "Open Settings",
  );
  if (action === "Open Settings") {
    await openMemoryOpsSettings();
  }
}

async function showSearchResults(results: MemorySearchResult[], title: string): Promise<void> {
  if (results.length === 0) {
    void vscode.window.showInformationMessage("MemoryOps returned no matching memories.");
    return;
  }

  const item = await vscode.window.showQuickPick(
    results.map((result, index) => ({
      label: result.content ? truncate(firstLine(result.content), 120) : `Memory ${index + 1}`,
      description: [
        result.pinned ? "pinned" : undefined,
        result.memory_type,
        result.scope_visibility,
        scoreLabel(result.score),
      ].filter(Boolean).join(" - "),
      detail: result.content ? truncate(result.content, 500) : JSON.stringify(result, null, 2),
      result,
    })),
    {
      title,
      matchOnDescription: true,
      matchOnDetail: true,
    },
  );

  if (!item) {
    return;
  }

  await openMarkdownDocument(formatMemoryResult(item.result));
}

async function showRetrievalResult(result: RetrievalResult, title: string): Promise<void> {
  const memories = Array.isArray(result.memories) ? result.memories : [];
  const markdown = [
    `# ${title}`,
    result.query_id ? `Query ID: \`${result.query_id}\`` : undefined,
    typeof result.total_tokens === "number" ? `Total tokens: ${result.total_tokens}` : undefined,
    "",
    result.packed_context ?? result.context,
    ...(memories.length > 0
      ? [
          "",
          "## Memories",
          ...memories.map((memory, index) => [
            `### ${index + 1}. ${memory.id ?? "Memory"}`,
            memory.score !== undefined ? `Score: ${memory.score}` : undefined,
            memory.importance_score !== undefined ? `Importance: ${memory.importance_score}` : undefined,
            memory.memory_type ? `Type: ${memory.memory_type}` : undefined,
            "",
            memory.content ?? "```json\n" + JSON.stringify(memory, null, 2) + "\n```",
          ].filter(Boolean).join("\n")),
        ]
      : []),
  ]
    .filter((part) => part !== undefined && part !== "")
    .join("\n");

  await openMarkdownDocument(markdown || "# MemoryOps Context\n\nNo context returned.");
}

function formatMemoryResult(result: MemorySearchResult): string {
  return [
    `# MemoryOps Memory ${result.id ? `\`${result.id}\`` : ""}`,
    result.score !== undefined ? `Score: ${result.score}` : undefined,
    result.memory_type ? `Type: ${result.memory_type}` : undefined,
    result.scope_visibility ? `Visibility: ${result.scope_visibility}` : undefined,
    result.pinned !== undefined ? `Pinned: ${result.pinned ? "yes" : "no"}` : undefined,
    result.importance_score !== undefined ? `Importance: ${result.importance_score}` : undefined,
    result.decay_score !== undefined ? `Decay: ${result.decay_score}` : undefined,
    result.relevance_score !== undefined ? `Relevance: ${result.relevance_score}` : undefined,
    Array.isArray(result.tags) && result.tags.length > 0 ? `Tags: ${result.tags.join(", ")}` : undefined,
    result.created_at ? `Created: ${result.created_at}` : undefined,
    result.updated_at ? `Updated: ${result.updated_at}` : undefined,
    "",
    result.content ?? "```json\n" + JSON.stringify(result, null, 2) + "\n```",
  ]
    .filter(Boolean)
    .join("\n");
}

async function openMarkdownDocument(markdown: string): Promise<void> {
  const document = await vscode.workspace.openTextDocument({
    language: "markdown",
    content: markdown,
  });
  await vscode.window.showTextDocument(document, { preview: true });
}

function selectedTextOrEmpty(editor: vscode.TextEditor): string {
  if (editor.selection.isEmpty) {
    return "";
  }
  return editor.document.getText(editor.selection);
}

function firstLine(value: string): string {
  return value.split(/\r?\n/)[0] ?? value;
}

function truncate(value: string, maxLength: number): string {
  return value.length <= maxLength ? value : `${value.slice(0, maxLength - 3)}...`;
}

function scoreLabel(score: unknown): string | undefined {
  return typeof score === "number" ? `score ${score.toFixed(3)}` : undefined;
}

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}