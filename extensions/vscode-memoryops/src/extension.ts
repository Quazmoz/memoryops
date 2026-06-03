import * as vscode from "vscode";
import * as fs from "fs";
import * as os from "os";
import * as path from "path";

import { MemoryOpsClient, MemorySearchResult } from "./client";
import { getConfig, openMemoryOpsSettings, setCachedApiKeySecret, validateConfig } from "./config";
import { registerChatParticipant } from "./chatParticipant";
import { MemoryCodeLensProvider } from "./codeLensProvider";
import { MemoryWebviewViewProvider } from "./webviewProvider";
import { memoryFromCommandArgument, memoryLabel } from "./memoryTree";
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
let memoryTreeProvider: MemoryWebviewViewProvider;
let outputChannel: vscode.OutputChannel;
let codeLensProvider: MemoryCodeLensProvider | undefined;

const WALKTHROUGH_ID = "quazmoz.memoryops-vscode#memoryops.gettingStarted";
const FIRST_RUN_KEY = "memoryops.hasSeenWalkthrough";

// Cached client instance — invalidated when config changes
let cachedClient: { client: MemoryOpsClient; config: ReturnType<typeof getConfig>; configKey: string } | undefined;

// Track active edit disposables so we don't leak listeners (keyed by memory ID)
const activeEditDisposables = new Map<string, vscode.Disposable[]>();

interface LoadRecentMemoriesOptions {
  append?: boolean;
  showProgress?: boolean;
  promptOnMissingConfig?: boolean;
}

export function activate(context: vscode.ExtensionContext): void {
  outputChannel = vscode.window.createOutputChannel("MemoryOps");
  context.subscriptions.push(outputChannel);

  memoryTreeProvider = new MemoryWebviewViewProvider(context.extensionUri);
  context.subscriptions.push(
    vscode.window.registerWebviewViewProvider("memoryops.memories", memoryTreeProvider)
  );

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
    vscode.commands.registerCommand("memoryops.editMemory", editMemory),
    vscode.commands.registerCommand("memoryops.submitFeedback", submitFeedback),
    vscode.commands.registerCommand("memoryops.insertContext", insertContext),
    vscode.commands.registerCommand("memoryops.copyContext", copyContext),
    vscode.commands.registerCommand("memoryops.setFilterSort", setFilterSort),
    vscode.commands.registerCommand("memoryops.searchMemoryInline", searchMemoryInline),
    vscode.commands.registerCommand("memoryops.editMemoryInline", editMemoryInline),
    vscode.commands.registerCommand("memoryops.submitFeedbackInline", submitFeedbackInline),
    vscode.commands.registerCommand("memoryops.mergeMemory", mergeMemory),
    vscode.commands.registerCommand("memoryops.bulkOperations", bulkOperations),
    vscode.commands.registerCommand("memoryops.reconnect", reconnect),
    vscode.commands.registerCommand("memoryops.showMemoriesForFile", showMemoriesForFile),
    vscode.commands.registerCommand("memoryops.openWalkthrough", openWalkthrough),
    vscode.commands.registerCommand("memoryops.setApiKey", async () => {
      const value = await vscode.window.showInputBox({
        title: "MemoryOps: Set API Key",
        prompt: "Enter your MemoryOps workspace API key. It will be stored securely in your OS keychain.",
        password: true,
        ignoreFocusOut: true,
      });
      if (value === undefined) {
        return;
      }
      if (value.trim()) {
        await context.secrets.store("memoryops.apiKey", value.trim());
        void vscode.window.showInformationMessage("MemoryOps API key stored securely.");
      } else {
        await context.secrets.delete("memoryops.apiKey");
        void vscode.window.showInformationMessage("MemoryOps API key removed from secure storage.");
      }
    }),
    vscode.workspace.onDidChangeConfiguration((event) => {
      if (!event.affectsConfiguration("memoryops")) {
        return;
      }
      cachedClient = undefined;
      codeLensProvider?.refresh();
      void initializeSidebar();
    }),
  );

  // Feature 9: inline CodeLens hints (gated on memoryops.enableCodeLens).
  codeLensProvider = new MemoryCodeLensProvider(() => getClient());
  context.subscriptions.push(
    codeLensProvider,
    vscode.languages.registerCodeLensProvider({ scheme: "file" }, codeLensProvider),
  );

  // Feature 8: @memoryops Copilot Chat participant (no-op if chat is unavailable).
  registerChatParticipant(context, () => getClient());

  // Listen for secure storage changes (e.g., API key set/cleared)
  context.subscriptions.push(
    context.secrets.onDidChange((event) => {
      if (event.key === "memoryops.apiKey") {
        void context.secrets.get("memoryops.apiKey").then((secret) => {
          setCachedApiKeySecret(secret || undefined);
          cachedClient = undefined;
          void initializeSidebar();
        });
      }
    })
  );

  // Read the secure API key before initializing the sidebar
  void context.secrets.get("memoryops.apiKey").then((secret) => {
    setCachedApiKeySecret(secret || undefined);
    void initializeSidebar();
    // Feature 5: on first install, open the Getting Started walkthrough so users
    // who install the extension and "see nothing" are guided through setup.
    // Runs after the secret loads so a stored key isn't mistaken for missing.
    maybeShowWalkthroughOnFirstRun(context);
  });
}

function maybeShowWalkthroughOnFirstRun(context: vscode.ExtensionContext): void {
  if (context.globalState.get<boolean>(FIRST_RUN_KEY)) {
    return;
  }
  void context.globalState.update(FIRST_RUN_KEY, true);

  // Only nudge users who haven't configured anything yet.
  if (validateConfig(getConfig()).length === 0) {
    return;
  }

  void vscode.commands.executeCommand(
    "workbench.action.openWalkthrough",
    WALKTHROUGH_ID,
    false,
  );
}

async function openWalkthrough(): Promise<void> {
  await vscode.commands.executeCommand(
    "workbench.action.openWalkthrough",
    WALKTHROUGH_ID,
    false,
  );
}

export function deactivate(): void {
  statusBarItem?.dispose();
  outputChannel?.dispose();
  // Clean up any lingering edit listeners
  for (const disposables of activeEditDisposables.values()) {
    for (const d of disposables) {
      d.dispose();
    }
  }
  activeEditDisposables.clear();
}

function disposeEditListeners(memoryId: string): void {
  const existing = activeEditDisposables.get(memoryId);
  if (existing) {
    for (const d of existing) {
      d.dispose();
    }
    activeEditDisposables.delete(memoryId);
  }
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
    statusBarItem.command = "memoryops.testConnection";
    void refreshMemories({ showProgress: false, promptOnMissingConfig: false });
    void vscode.window.showInformationMessage("MemoryOps connection is healthy.");
  } catch (error) {
    statusBarItem.text = "$(error) MemoryOps";
    statusBarItem.tooltip = `MemoryOps connection failed: ${errorMessage(error)} — click to reconnect`;
    statusBarItem.command = "memoryops.reconnect";
    void vscode.window
      .showErrorMessage(`MemoryOps connection failed: ${errorMessage(error)}`, "Reconnect", "Open Settings")
      .then((action) => {
        if (action === "Reconnect") {
          void reconnect();
        } else if (action === "Open Settings") {
          void openMemoryOpsSettings();
        }
      });
    throw error;
  }
}

async function reconnect(): Promise<void> {
  // Drop the cached client so fresh config/secrets are picked up, then re-run
  // the connection check. Transient failures are retried inside the client.
  cachedClient = undefined;
  setDefaultStatusBar();
  await testConnection();
}

async function showMemoriesForFile(fileNameArg?: unknown): Promise<void> {
  const { client, config, missing } = getClient();
  if (missing.length > 0) {
    setIncompleteStatusBar(missing);
    await promptForMissingConfig(missing);
    return;
  }

  const document = vscode.window.activeTextEditor?.document;
  const fileName = typeof fileNameArg === "string" && fileNameArg
    ? fileNameArg
    : document
      ? getRelativeFileName(document)
      : undefined;

  if (!fileName) {
    void vscode.window.showWarningMessage("Open a file to find memories that reference it.");
    return;
  }

  const response = await vscode.window.withProgress(
    {
      location: vscode.ProgressLocation.Notification,
      title: `Finding MemoryOps memories for ${fileName}...`,
      cancellable: false,
    },
    // Precise: filter memories by the source file recorded on their originating
    // observation, rather than a fuzzy full-text search on the file name.
    () => client.listMemory({
      sourceRef: fileName,
      limit: config.sidebarPageSize,
      sort: memoryTreeProvider.getSortField(),
      direction: memoryTreeProvider.getSortDirection(),
    }),
  );

  const results = response.items as MemorySearchResult[];
  memoryTreeProvider.setSearchResults(results, fileName);
  await showSearchResults(results, `MemoryOps memories referencing ${fileName}`);
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
        pinned: memoryTreeProvider.getFilterPinned(),
        memoryType: memoryTreeProvider.getFilterType(),
        sort: memoryTreeProvider.getSortField(),
        direction: memoryTreeProvider.getSortDirection(),
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
    if (result.query_id) {
      for (const m of result.memories) {
        m.query_id = result.query_id;
      }
    }
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
  const configKey = `${config.apiUrl}|${config.workspaceId}|${config.apiKey}`;

  if (!cachedClient || cachedClient.configKey !== configKey) {
    cachedClient = {
      client: new MemoryOpsClient(config, (msg) => outputChannel.appendLine(`[${new Date().toISOString()}] ${msg}`)),
      config,
      configKey,
    };
  }

  return {
    config: cachedClient.config,
    client: cachedClient.client,
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
  if (typeof item === "string") {
    const memories = memoryTreeProvider.getMemories();
    const found = memories.find((m) => m.id === item);
    if (found) {
      return found;
    }
  }

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

async function editMemory(item?: unknown): Promise<void> {
  const memory = await resolveMemorySelection(item, "MemoryOps: Edit Memory");
  if (!memory?.id) {
    return;
  }

  const option = await vscode.window.showQuickPick(
    ["📝 Edit Content", "🏷️ Edit Tags", "🔥 Edit Importance Score"],
    { title: `Edit Memory: ${memoryLabel(memory)}` }
  );

  if (!option) {
    return;
  }

  const { client, missing } = getClient();
  if (missing.length > 0) {
    await promptForMissingConfig(missing);
    return;
  }

  if (option === "📝 Edit Content") {
    const memoryId = memory.id!;
    const tmpPath = path.join(os.tmpdir(), `memoryops-edit-${memoryId}.md`);

    // Dispose any previous edit listeners for this memory
    disposeEditListeners(memoryId);

    try {
      fs.writeFileSync(tmpPath, memory.content ?? "");
    } catch (err) {
      void vscode.window.showErrorMessage(`Failed to create temp file: ${errorMessage(err)}`);
      return;
    }

    const document = await vscode.workspace.openTextDocument(tmpPath);
    await vscode.window.showTextDocument(document);

    const saveDisposable = vscode.workspace.onDidSaveTextDocument(async (doc) => {
      if (doc.fileName === tmpPath) {
        const updatedContent = doc.getText();
        try {
          await vscode.window.withProgress(
            {
              location: vscode.ProgressLocation.Notification,
              title: "Updating MemoryOps memory content...",
              cancellable: false,
            },
            async () => {
              const updated = await client.updateMemory(memoryId, { content: updatedContent });
              memoryTreeProvider.updateMemory(updated);
            }
          );
          void vscode.window.showInformationMessage("MemoryOps memory content updated.");
        } catch (err) {
          void vscode.window.showErrorMessage(`Failed to update memory: ${errorMessage(err)}`);
        }
      }
    });

    const closeDisposable = vscode.workspace.onDidCloseTextDocument((doc) => {
      if (doc.fileName === tmpPath) {
        disposeEditListeners(memoryId);
        try {
          fs.unlinkSync(tmpPath);
        } catch {}
      }
    });

    activeEditDisposables.set(memoryId, [saveDisposable, closeDisposable]);
  } else if (option === "🏷️ Edit Tags") {
    const currentTags = memory.tags?.join(", ") ?? "";
    const updatedTagsInput = await vscode.window.showInputBox({
      title: "Edit Memory Tags",
      prompt: "Enter comma-separated tags.",
      value: currentTags,
      ignoreFocusOut: true,
    });

    if (updatedTagsInput === undefined) {
      return;
    }

    const tags = updatedTagsInput
      .split(",")
      .map((tag) => tag.trim())
      .filter((tag) => tag.length > 0);

    const updated = await vscode.window.withProgress(
      {
        location: vscode.ProgressLocation.Notification,
        title: "Updating MemoryOps memory tags...",
        cancellable: false,
      },
      () => client.updateMemory(memory.id!, { tags })
    );

    memoryTreeProvider.updateMemory(updated);
    void vscode.window.showInformationMessage("MemoryOps memory tags updated.");
  } else if (option === "🔥 Edit Importance Score") {
    const currentImportance = memory.importance_score?.toString() ?? "0.5";
    const updatedImportanceInput = await vscode.window.showInputBox({
      title: "Edit Memory Importance Score",
      prompt: "Enter a score between 0.0 and 1.0.",
      value: currentImportance,
      ignoreFocusOut: true,
      validateInput: (value) => {
        const num = parseFloat(value);
        if (isNaN(num) || num < 0.0 || num > 1.0) {
          return "Please enter a number between 0.0 and 1.0.";
        }
        return null;
      },
    });

    if (updatedImportanceInput === undefined) {
      return;
    }

    const importanceScore = parseFloat(updatedImportanceInput);
    const updated = await vscode.window.withProgress(
      {
        location: vscode.ProgressLocation.Notification,
        title: "Updating MemoryOps memory importance score...",
        cancellable: false,
      },
      () => client.updateMemory(memory.id!, { importance_score: importanceScore })
    );

    memoryTreeProvider.updateMemory(updated);
    void vscode.window.showInformationMessage("MemoryOps memory importance score updated.");
  }
}

async function mergeMemory(item?: unknown): Promise<void> {
  const source = await resolveMemorySelection(item, "MemoryOps: Merge Memory — Select Source");
  if (!source?.id) {
    void vscode.window.showWarningMessage("Select a MemoryOps memory to merge from.");
    return;
  }

  if (source.memory_type && source.memory_type !== "semantic") {
    void vscode.window.showWarningMessage("Only semantic memories can be merged. Promote this memory first.");
    return;
  }

  const memories = memoryTreeProvider.getMemories();
  const candidates = memories.filter((m) => m.id && m.id !== source.id && (!m.memory_type || m.memory_type === "semantic"));

  if (candidates.length === 0) {
    void vscode.window.showWarningMessage("No other semantic memories available to merge with.");
    return;
  }

  const targetPick = await vscode.window.showQuickPick(
    candidates.map((memory) => ({
      label: memoryLabel(memory),
      description: [memory.memory_type, memory.scope_visibility].filter(Boolean).join(" — "),
      detail: memory.content ? truncate(firstLine(memory.content), 160) : undefined,
      memory,
    })),
    {
      title: "MemoryOps: Merge Memory — Select Target",
      placeHolder: "Select the target memory to merge into",
      matchOnDescription: true,
      matchOnDetail: true,
    },
  );

  if (!targetPick) {
    return;
  }

  const target = targetPick.memory;

  const confirmed = await vscode.window.showWarningMessage(
    `Merge "${truncate(firstLine(source.content ?? source.id ?? "source"), 50)}" into "${truncate(firstLine(target.content ?? target.id ?? "target"), 50)}"? The source memory will be removed.`,
    { modal: true },
    "Merge",
  );

  if (confirmed !== "Merge") {
    return;
  }

  const { client, missing } = getClient();
  if (missing.length > 0) {
    await promptForMissingConfig(missing);
    return;
  }

  const merged = await vscode.window.withProgress(
    {
      location: vscode.ProgressLocation.Notification,
      title: "Merging MemoryOps memories...",
      cancellable: false,
    },
    () => client.mergeMemory(source.id!, target.id!),
  );

  memoryTreeProvider.removeMemory(source.id);
  memoryTreeProvider.updateMemory(merged);

  const action = await vscode.window.showInformationMessage("MemoryOps memories merged successfully.", "Open Merged Memory");
  if (action === "Open Merged Memory") {
    await openMarkdownDocument(formatMemoryMarkdown(merged));
  }
}

async function bulkOperations(): Promise<void> {
  const { client, missing } = getClient();
  if (missing.length > 0) {
    await promptForMissingConfig(missing);
    return;
  }

  const memories = memoryTreeProvider.getMemories();
  if (memories.length === 0) {
    void vscode.window.showWarningMessage("Load or search MemoryOps memories first.");
    return;
  }

  const selections = await vscode.window.showQuickPick(
    memories.filter((m) => m.id).map((memory) => ({
      label: memoryLabel(memory),
      description: [
        memory.pinned ? "📌 pinned" : undefined,
        memory.memory_type,
        memory.scope_visibility,
      ].filter(Boolean).join(" — "),
      detail: memory.content ? truncate(firstLine(memory.content), 160) : undefined,
      memory,
      picked: false,
    })),
    {
      title: "MemoryOps: Bulk Operations — Select Memories",
      placeHolder: "Select memories to operate on",
      canPickMany: true,
      matchOnDescription: true,
      matchOnDetail: true,
    },
  );

  if (!selections || selections.length === 0) {
    return;
  }

  const ids = selections.map((s) => s.memory.id!).filter((id) => id);

  const operation = await vscode.window.showQuickPick(
    [
      { label: "📌 Pin Selected", value: "pin" as const },
      { label: "📍 Unpin Selected", value: "unpin" as const },
      { label: "🗑️ Delete Selected", value: "delete" as const },
    ],
    {
      title: `MemoryOps: Bulk Operation — ${ids.length} memories selected`,
      placeHolder: "Choose an operation",
    },
  );

  if (!operation) {
    return;
  }

  if (operation.value === "delete") {
    const confirmed = await vscode.window.showWarningMessage(
      `Delete ${ids.length} MemoryOps memories? This cannot be undone easily.`,
      { modal: true },
      "Delete All",
    );
    if (confirmed !== "Delete All") {
      return;
    }
  }

  const result = await vscode.window.withProgress(
    {
      location: vscode.ProgressLocation.Notification,
      title: `MemoryOps: ${operation.label.replace(/^\S+\s/, "")}...`,
      cancellable: false,
    },
    () => client.bulkOperation(ids, operation.value),
  );

  if (operation.value === "delete") {
    for (const id of ids) {
      memoryTreeProvider.removeMemory(id);
    }
  } else {
    void refreshMemories();
  }

  void vscode.window.showInformationMessage(
    `MemoryOps bulk ${operation.value}: ${result.affected} memories affected.`,
  );
}

async function submitFeedback(item?: unknown): Promise<void> {
  const memory = await resolveMemorySelection(item, "MemoryOps: Submit Feedback");
  if (!memory?.id) {
    return;
  }

  const queryId = memory.query_id ?? (memory as Record<string, unknown>)["queryId"];
  if (typeof queryId !== "string" || !queryId) {
    void vscode.window.showWarningMessage("Feedback can only be submitted for retrieved memories or search results.");
    return;
  }

  const ratingSelection = await vscode.window.showQuickPick(
    [
      { label: "👍 Helpful (+1)", value: 1 },
      { label: "😐 Neutral (0)", value: 0 },
      { label: "👎 Not Helpful (-1)", value: -1 }
    ],
    { title: `Rate Memory Relevance: ${memoryLabel(memory)}` }
  );

  if (!ratingSelection) {
    return;
  }

  const comment = await vscode.window.showInputBox({
    title: "Submit Feedback Comment",
    prompt: "Optional comment explaining your rating (max 500 characters).",
    ignoreFocusOut: true,
    validateInput: (value) => {
      if (value.length > 500) {
        return "Comment must be under 500 characters.";
      }
      return null;
    }
  });

  if (comment === undefined) {
    return;
  }

  const { client, config, missing } = getClient();
  if (missing.length > 0) {
    await promptForMissingConfig(missing);
    return;
  }

  await vscode.window.withProgress(
    {
      location: vscode.ProgressLocation.Notification,
      title: "Submitting MemoryOps feedback...",
      cancellable: false,
    },
    () => client.submitMemoryFeedback(memory.id!, {
      queryId,
      rating: ratingSelection.value,
      comment: comment.trim() || null,
      agentId: config.defaultAgentId,
    })
  );

  void vscode.window.showInformationMessage("MemoryOps feedback submitted successfully.");
}

async function getRetrievalContextHelper(): Promise<string | undefined> {
  const { client, config, missing } = getClient();
  if (missing.length > 0) {
    setIncompleteStatusBar(missing);
    await promptForMissingConfig(missing);
    return undefined;
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
    if (result.query_id) {
      for (const m of result.memories) {
        m.query_id = result.query_id;
      }
    }
    memoryTreeProvider.setRetrievedMemories(result.memories, "No memories returned for current context.");
  }

  return result.packed_context ?? result.context;
}

async function insertContext(): Promise<void> {
  const contextText = await getRetrievalContextHelper();
  if (!contextText) {
    return;
  }

  const editor = vscode.window.activeTextEditor;
  if (!editor) {
    void vscode.window.showWarningMessage("Open a text file to insert retrieval context.");
    return;
  }

  await editor.edit((editBuilder) => {
    editBuilder.insert(editor.selection.active, contextText);
  });
  void vscode.window.showInformationMessage("MemoryOps retrieval context inserted at cursor.");
}

async function copyContext(): Promise<void> {
  const contextText = await getRetrievalContextHelper();
  if (!contextText) {
    return;
  }

  await vscode.env.clipboard.writeText(contextText);
  void vscode.window.showInformationMessage("MemoryOps retrieval context copied to clipboard.");
}

async function setFilterSort(): Promise<void> {
  const option = await vscode.window.showQuickPick(
    [
      "🔍 Filter by Memory Type",
      "📌 Filter by Pinned Status",
      "↕️ Sort Field",
      "🔀 Sort Direction",
      "🔄 Reset Filters and Sorting"
    ],
    { title: "MemoryOps: Filter and Sort Sidebar" }
  );

  if (!option) {
    return;
  }

  if (option === "🔍 Filter by Memory Type") {
    const typeOption = await vscode.window.showQuickPick(
      [
        { label: "All memories", value: undefined },
        { label: "Episodic only", value: "episodic" as const },
        { label: "Semantic only", value: "semantic" as const }
      ],
      { title: "Filter by Memory Type" }
    );
    if (typeOption) {
      memoryTreeProvider.setFilterType(typeOption.value);
      void refreshMemories();
    }
  } else if (option === "📌 Filter by Pinned Status") {
    const pinnedOption = await vscode.window.showQuickPick(
      [
        { label: "All memories", value: undefined },
        { label: "Pinned only", value: true },
        { label: "Unpinned only", value: false }
      ],
      { title: "Filter by Pinned Status" }
    );
    if (pinnedOption) {
      memoryTreeProvider.setFilterPinned(pinnedOption.value);
      void refreshMemories();
    }
  } else if (option === "↕️ Sort Field") {
    const sortOption = await vscode.window.showQuickPick(
      [
        { label: "Updated time", value: "updated_at" as const },
        { label: "Created time", value: "created_at" as const },
        { label: "Importance score", value: "importance_score" as const },
        { label: "Decay score", value: "decay_score" as const },
        { label: "Relevance score", value: "relevance_score" as const }
      ],
      { title: "Sort Field" }
    );
    if (sortOption) {
      memoryTreeProvider.setSortField(sortOption.value);
      void refreshMemories();
    }
  } else if (option === "🔀 Sort Direction") {
    const dirOption = await vscode.window.showQuickPick(
      [
        { label: "Descending (newest/highest first)", value: "desc" as const },
        { label: "Ascending (oldest/lowest first)", value: "asc" as const }
      ],
      { title: "Sort Direction" }
    );
    if (dirOption) {
      memoryTreeProvider.setSortDirection(dirOption.value);
      void refreshMemories();
    }
  } else if (option === "🔄 Reset Filters and Sorting") {
    memoryTreeProvider.setFilterType(undefined);
    memoryTreeProvider.setFilterPinned(undefined);
    memoryTreeProvider.setSortField("updated_at");
    memoryTreeProvider.setSortDirection("desc");
    void refreshMemories();
    void vscode.window.showInformationMessage("Filters and sorting reset to defaults.");
  }
}

async function searchMemoryInline(query: string): Promise<void> {
  const { client, config, missing } = getClient();
  if (missing.length > 0) {
    memoryTreeProvider.setMessage(`Configure MemoryOps settings to search: ${missing.join(", ")}`);
    return;
  }

  if (!query.trim()) {
    // Skip redundant refresh if we're already showing recent memories
    if (memoryTreeProvider.getMode() !== "recent") {
      void refreshMemories({ showProgress: false, promptOnMissingConfig: false });
    }
    return;
  }

  try {
    const repo = await getWorkspaceRepoHint(vscode.window.activeTextEditor?.document);
    const results = await client.searchMemory(query.trim(), config.defaultTopK, {
      mode: config.defaultSearchMode,
      repo,
      includeWorkspacePool: config.includeWorkspacePool,
    });

    memoryTreeProvider.setSearchResults(results, query.trim());
  } catch (error) {
    memoryTreeProvider.setError(errorMessage(error));
  }
}

async function editMemoryInline(id: string, field: string): Promise<void> {
  await editMemoryInlineHelper(id, field);
}

async function editMemoryInlineHelper(id: string, field: string): Promise<void> {
  const memories = memoryTreeProvider.getMemories();
  const memory = memories.find((m) => m.id === id);
  if (!memory) {
    return;
  }

  let option = field;
  if (field === "all") {
    const selection = await vscode.window.showQuickPick(
      ["📝 Edit Content", "🏷️ Edit Tags", "🔥 Edit Importance Score"],
      { title: `Edit Memory: ${memoryLabel(memory)}` }
    );
    if (!selection) {
      return;
    }
    option = selection;
  }

  const { client, missing } = getClient();
  if (missing.length > 0) {
    await promptForMissingConfig(missing);
    return;
  }

  if (option === "📝 Edit Content" || option === "content") {
    const memoryId = memory.id!;
    const tmpPath = path.join(os.tmpdir(), `memoryops-edit-${memoryId}.md`);

    // Dispose any previous edit listeners for this memory
    disposeEditListeners(memoryId);

    try {
      fs.writeFileSync(tmpPath, memory.content ?? "");
    } catch (err) {
      void vscode.window.showErrorMessage(`Failed to create temp file: ${errorMessage(err)}`);
      return;
    }

    const document = await vscode.workspace.openTextDocument(tmpPath);
    await vscode.window.showTextDocument(document);

    const saveDisposable = vscode.workspace.onDidSaveTextDocument(async (doc) => {
      if (doc.fileName === tmpPath) {
        const updatedContent = doc.getText();
        try {
          await vscode.window.withProgress(
            {
              location: vscode.ProgressLocation.Notification,
              title: "Updating MemoryOps memory content...",
              cancellable: false,
            },
            async () => {
              const updated = await client.updateMemory(memoryId, { content: updatedContent });
              memoryTreeProvider.updateMemory(updated);
            }
          );
          void vscode.window.showInformationMessage("MemoryOps memory content updated.");
        } catch (err) {
          void vscode.window.showErrorMessage(`Failed to update memory: ${errorMessage(err)}`);
        }
      }
    });

    const closeDisposable = vscode.workspace.onDidCloseTextDocument((doc) => {
      if (doc.fileName === tmpPath) {
        disposeEditListeners(memoryId);
        try {
          fs.unlinkSync(tmpPath);
        } catch {}
      }
    });

    activeEditDisposables.set(memoryId, [saveDisposable, closeDisposable]);
  } else if (option === "🏷️ Edit Tags" || option === "tags") {
    const currentTags = memory.tags?.join(", ") ?? "";
    const updatedTagsInput = await vscode.window.showInputBox({
      title: "Edit Memory Tags",
      prompt: "Enter comma-separated tags.",
      value: currentTags,
      ignoreFocusOut: true,
    });

    if (updatedTagsInput === undefined) {
      return;
    }

    const tags = updatedTagsInput
      .split(",")
      .map((tag) => tag.trim())
      .filter((tag) => tag.length > 0);

    const updated = await vscode.window.withProgress(
      {
        location: vscode.ProgressLocation.Notification,
        title: "Updating MemoryOps memory tags...",
        cancellable: false,
      },
      () => client.updateMemory(memory.id!, { tags })
    );

    memoryTreeProvider.updateMemory(updated);
    void vscode.window.showInformationMessage("MemoryOps memory tags updated.");
  } else if (option === "🔥 Edit Importance Score" || option === "importance") {
    const currentImportance = memory.importance_score?.toString() ?? "0.5";
    const updatedImportanceInput = await vscode.window.showInputBox({
      title: "Edit Memory Importance Score",
      prompt: "Enter a score between 0.0 and 1.0.",
      value: currentImportance,
      ignoreFocusOut: true,
      validateInput: (value) => {
        const num = parseFloat(value);
        if (isNaN(num) || num < 0.0 || num > 1.0) {
          return "Please enter a number between 0.0 and 1.0.";
        }
        return null;
      },
    });

    if (updatedImportanceInput === undefined) {
      return;
    }

    const importanceScore = parseFloat(updatedImportanceInput);
    const updated = await vscode.window.withProgress(
      {
        location: vscode.ProgressLocation.Notification,
        title: "Updating MemoryOps memory importance score...",
        cancellable: false,
      },
      () => client.updateMemory(memory.id!, { importance_score: importanceScore })
    );

    memoryTreeProvider.updateMemory(updated);
    void vscode.window.showInformationMessage("MemoryOps memory importance score updated.");
  }
}

async function submitFeedbackInline(
  id: string,
  payload: { queryId: string; rating: number; comment?: string }
): Promise<void> {
  const { client, config, missing } = getClient();
  if (missing.length > 0) {
    await promptForMissingConfig(missing);
    return;
  }

  try {
    await vscode.window.withProgress(
      {
        location: vscode.ProgressLocation.Notification,
        title: "Submitting MemoryOps feedback...",
        cancellable: false,
      },
      () => client.submitMemoryFeedback(id, {
        queryId: payload.queryId,
        rating: payload.rating,
        comment: payload.comment || null,
        agentId: config.defaultAgentId,
      })
    );

    void vscode.window.showInformationMessage("MemoryOps feedback submitted successfully.");
  } catch (err) {
    void vscode.window.showErrorMessage(`Failed to submit feedback: ${errorMessage(err)}`);
  }
}