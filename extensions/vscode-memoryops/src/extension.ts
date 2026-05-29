import * as vscode from "vscode";

import { MemoryOpsClient, MemorySearchResult } from "./client";
import { getConfig, openMemoryOpsSettings, validateConfig } from "./config";
import { MemoryTreeProvider, memoryFromCommandArgument, memoryLabel } from "./memoryTree";
import {
  errorMessage,
  firstLine,
  formatMemoryFeedbackMarkdown,
  formatMemoryHistoryMarkdown,
  formatMemoryMarkdown,
  formatMemoryProvenanceMarkdown,
  formatRetrievalMarkdown,
  scoreLabel,
  truncate,
} from "./markdown";
import { getRelativeFileName, getSourceRef, getWorkspaceRepoHint } from "./repo";

let statusBarItem: vscode.StatusBarItem;
let memoryTreeProvider: MemoryTreeProvider;

interface LoadRecentMemoriesOptions {
  append?: boolean;
  showProgress?: boolean;
  promptOnMissingConfig?: boolean;
}

export function activate(context: vscode.ExtensionContext): void {
  memoryTreeProvider = new MemoryTreeProvider();
  context.subscriptions.push(vscode.window.registerTreeDataProvider("memoryops.memories", memoryTreeProvider));

  statusBarItem = vscode.window.createStatusBarItem(vscode.StatusBarAlignment.Left, 100);
  statusBarItem.command = "memoryops.testConnection";
  setDefaultStatusBar();
  statusBarItem.show();
  context.subscriptions.push(statusBarItem);

  context.subscriptions.push(
    vscode.commands.registerCommand("memoryops.testConnection", testConnection),
    vscode.commands.registerCommand("memoryops.refreshMemories", refreshMemories),
    vscode.commands.registerCommand("memoryops.loadMoreMemories", loadMoreMemories),
    vscode.commands.registerCommand("memoryops.searchMemory", searchMemory),
    vscode.commands.registerCommand("memoryops.retrieveContextForCurrentFile", retrieveContextForCurrentFile),
    vscode.commands.registerCommand("memoryops.saveSelectionAsObservation", saveSelectionAsObservation),
    vscode.commands.registerCommand("memoryops.openMemory", openMemory),
    vscode.commands.registerCommand("memoryops.viewMemoryHistory", viewMemoryHistory),
    vscode.commands.registerCommand("memoryops.viewMemoryProvenance", viewMemoryProvenance),
    vscode.commands.registerCommand("memoryops.viewMemoryFeedback", viewMemoryFeedback),
    vscode.commands.registerCommand("memoryops.promoteMemory", promoteMemory),
    vscode.commands.registerCommand("memoryops.publishMemory", publishMemory),
    vscode.commands.registerCommand("memoryops.pinMemory", (item?: unknown) => setMemoryPinned(item, true)),
    vscode.commands.registerCommand("memoryops.unpinMemory", (item?: unknown) => setMemoryPinned(item, false)),
    vscode.commands.registerCommand("memoryops.deleteMemory", deleteMemory),
    vscode.commands.registerCommand("memoryops.copyMemory", copyMemory),
    vscode.commands.registerCommand("memoryops.openSettings", openMemoryOpsSettings),
    vscode.workspace.onDidChangeConfiguration((event) => {
      if (!event.affectsConfiguration("memoryops")) {
        return;
      }
      void initializeSidebar();
    }),
  );

  void initializeSidebar();
}

export function deactivate(): void {
  statusBarItem?.dispose();
}

async function testConnection(): Promise<void> {
  const { client, missing } = getClient();
  if (missing.length > 0) {
    setIncompleteStatusBar(missing);
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
    void refreshMemories({ showProgress: false, promptOnMissingConfig: false });
    void vscode.window.showInformationMessage("MemoryOps connection is healthy.");
  } catch (error) {
    statusBarItem.text = "$(error) MemoryOps";
    statusBarItem.tooltip = `MemoryOps connection failed: ${errorMessage(error)}`;
    throw error;
  }
}

async function refreshMemories(options: LoadRecentMemoriesOptions = {}): Promise<void> {
  const append = options.append ?? false;
  const showProgress = options.showProgress ?? true;
  const promptOnMissingConfig = options.promptOnMissingConfig ?? true;
  const { client, config, missing } = getClient();
  if (missing.length > 0) {
    setIncompleteStatusBar(missing);
    if (promptOnMissingConfig) {
      await promptForMissingConfig(missing);
    } else {
      memoryTreeProvider.setMessage("Configure MemoryOps settings to load recent memories.");
    }
    return;
  }

  const offset = append ? memoryTreeProvider.getNextRecentOffset() : 0;
  if (append && offset === undefined) {
    void vscode.window.showInformationMessage("MemoryOps has no additional recent memories to load.");
    return;
  }

  try {
    setDefaultStatusBar();
    const load = () => client.listMemory({
        limit: config.sidebarPageSize,
        offset: offset ?? 0,
        sort: "updated_at",
        direction: "desc",
      });

    const response = showProgress
      ? await vscode.window.withProgress(
          {
            location: vscode.ProgressLocation.Window,
            title: append ? "MemoryOps: loading more memories..." : "MemoryOps: refreshing memories...",
            cancellable: false,
          },
          load,
        )
      : await load();

    memoryTreeProvider.setRecentMemories(response, { append });
  } catch (error) {
    if (!append) {
      memoryTreeProvider.setError(errorMessage(error));
    }
    throw error;
  }
}

async function loadMoreMemories(): Promise<void> {
  await refreshMemories({ append: true });
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
    setIncompleteStatusBar(missing);
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
  await openMarkdownDocument(formatRetrievalMarkdown(result, "MemoryOps Context"));
}

async function saveSelectionAsObservation(): Promise<void> {
  const { client, config, missing } = getClient();
  if (missing.length > 0) {
    setIncompleteStatusBar(missing);
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
  const memory = await resolveMemorySelection(item, "MemoryOps: Open Memory");
  if (!memory) {
    return;
  }

  await openMarkdownDocument(formatMemoryMarkdown(memory));
}

async function viewMemoryHistory(item?: unknown): Promise<void> {
  const memory = await resolveMemorySelection(item, "MemoryOps: View Memory History");
  if (!memory?.id) {
    void vscode.window.showWarningMessage("Select a MemoryOps memory first.");
    return;
  }
  const memoryId = memory.id;

  const { client, missing } = getClient();
  if (missing.length > 0) {
    setIncompleteStatusBar(missing);
    await promptForMissingConfig(missing);
    return;
  }

  const versions = await vscode.window.withProgress(
    {
      location: vscode.ProgressLocation.Notification,
      title: "Loading MemoryOps memory history...",
      cancellable: false,
    },
    () => client.getMemoryHistory(memoryId),
  );

  await openMarkdownDocument(formatMemoryHistoryMarkdown(memory, versions));
}

async function viewMemoryProvenance(item?: unknown): Promise<void> {
  const memory = await resolveMemorySelection(item, "MemoryOps: View Memory Provenance");
  if (!memory?.id) {
    void vscode.window.showWarningMessage("Select a MemoryOps memory first.");
    return;
  }
  const memoryId = memory.id;

  const { client, missing } = getClient();
  if (missing.length > 0) {
    setIncompleteStatusBar(missing);
    await promptForMissingConfig(missing);
    return;
  }

  const provenance = await vscode.window.withProgress(
    {
      location: vscode.ProgressLocation.Notification,
      title: "Loading MemoryOps provenance...",
      cancellable: false,
    },
    () => client.getMemoryProvenance(memoryId),
  );

  await openMarkdownDocument(formatMemoryProvenanceMarkdown(memory, provenance));
}

async function viewMemoryFeedback(item?: unknown): Promise<void> {
  const memory = await resolveMemorySelection(item, "MemoryOps: View Memory Feedback");
  if (!memory?.id) {
    void vscode.window.showWarningMessage("Select a MemoryOps memory first.");
    return;
  }
  const memoryId = memory.id;

  const { client, missing } = getClient();
  if (missing.length > 0) {
    setIncompleteStatusBar(missing);
    await promptForMissingConfig(missing);
    return;
  }

  const feedback = await vscode.window.withProgress(
    {
      location: vscode.ProgressLocation.Notification,
      title: "Loading MemoryOps feedback...",
      cancellable: false,
    },
    () => client.getMemoryFeedback(memoryId, { limit: 25, offset: 0 }),
  );

  await openMarkdownDocument(formatMemoryFeedbackMarkdown(memory, feedback));
}

async function promoteMemory(item?: unknown): Promise<void> {
  const memory = await resolveMemorySelection(item, "MemoryOps: Promote Memory");
  if (!memory?.id) {
    void vscode.window.showWarningMessage("Select a MemoryOps memory first.");
    return;
  }
  const memoryId = memory.id;

  if (memory.memory_type === "semantic") {
    void vscode.window.showInformationMessage("Selected MemoryOps memory is already semantic.");
    return;
  }

  const { client, missing } = getClient();
  if (missing.length > 0) {
    setIncompleteStatusBar(missing);
    await promptForMissingConfig(missing);
    return;
  }

  const updated = await vscode.window.withProgress(
    {
      location: vscode.ProgressLocation.Notification,
      title: "Promoting MemoryOps memory...",
      cancellable: false,
    },
    () => client.promoteMemory(memoryId),
  );

  memoryTreeProvider.updateMemory(updated);
  const action = await vscode.window.showInformationMessage("MemoryOps memory promoted to semantic.", "Open");
  if (action === "Open") {
    await openMarkdownDocument(formatMemoryMarkdown(updated));
  }
}

async function publishMemory(item?: unknown): Promise<void> {
  const memory = await resolveMemorySelection(item, "MemoryOps: Publish Memory");
  if (!memory?.id) {
    void vscode.window.showWarningMessage("Select a MemoryOps memory first.");
    return;
  }
  const memoryId = memory.id;

  if (memory.memory_type && memory.memory_type !== "semantic") {
    void vscode.window.showWarningMessage("Only semantic memories can be published to the workspace pool.");
    return;
  }

  if (memory.scope_visibility === "workspace") {
    void vscode.window.showInformationMessage("Selected MemoryOps memory is already published to the workspace pool.");
    return;
  }

  const { client, missing } = getClient();
  if (missing.length > 0) {
    setIncompleteStatusBar(missing);
    await promptForMissingConfig(missing);
    return;
  }

  const updated = await vscode.window.withProgress(
    {
      location: vscode.ProgressLocation.Notification,
      title: "Publishing MemoryOps memory...",
      cancellable: false,
    },
    () => client.publishMemory(memoryId),
  );

  memoryTreeProvider.updateMemory(updated);
  const action = await vscode.window.showInformationMessage("MemoryOps memory published to the workspace pool.", "Open");
  if (action === "Open") {
    await openMarkdownDocument(formatMemoryMarkdown(updated));
  }
}

async function setMemoryPinned(item: unknown, pinned: boolean): Promise<void> {
  const memory = await resolveMemorySelection(item, pinned ? "MemoryOps: Pin Memory" : "MemoryOps: Unpin Memory");
  if (!memory?.id) {
    void vscode.window.showWarningMessage("Select a MemoryOps memory first.");
    return;
  }

  const { client, missing } = getClient();
  if (missing.length > 0) {
    setIncompleteStatusBar(missing);
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
  const memory = await resolveMemorySelection(item, "MemoryOps: Delete Memory");
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
    setIncompleteStatusBar(missing);
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
  const memory = await resolveMemorySelection(item, "MemoryOps: Copy Memory Content");
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

  await openMarkdownDocument(formatMemoryMarkdown(item.result));
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

async function initializeSidebar(): Promise<void> {
  const missing = validateConfig(getConfig());
  if (missing.length > 0) {
    setIncompleteStatusBar(missing);
    memoryTreeProvider.setMessage("Configure MemoryOps settings to load recent memories.");
    return;
  }

  setDefaultStatusBar();
  try {
    await refreshMemories({ showProgress: false, promptOnMissingConfig: false });
  } catch {
    // refreshMemories already updates the tree provider state.
  }
}

async function resolveMemorySelection(item: unknown, title: string): Promise<MemorySearchResult | undefined> {
  const direct = memoryFromCommandArgument(item);
  if (direct) {
    return direct;
  }

  const memories = memoryTreeProvider.getMemories();
  if (memories.length === 0) {
    void vscode.window.showWarningMessage("Load or search MemoryOps memories first.");
    return undefined;
  }

  const selected = await vscode.window.showQuickPick(
    memories.map((memory, index) => ({
      label: memoryLabel(memory),
      description: [memory.memory_type, memory.scope_visibility, scoreLabel(memory.score)].filter(Boolean).join(" - "),
      detail: memory.content ? truncate(firstLine(memory.content), 160) : `Memory ${index + 1}`,
      memory,
    })),
    {
      title,
      matchOnDescription: true,
      matchOnDetail: true,
    },
  );

  return selected?.memory;
}

function setDefaultStatusBar(): void {
  statusBarItem.text = "$(database) MemoryOps";
  statusBarItem.tooltip = "MemoryOps: Test connection";
}

function setIncompleteStatusBar(missing: string[]): void {
  statusBarItem.text = "$(warning) MemoryOps";
  statusBarItem.tooltip = `MemoryOps settings are incomplete: ${missing.join(", ")}`;
}