import * as vscode from "vscode";

import { MemoryOpsClient, MemorySearchResult, RetrievalResult } from "./client";
import { getConfig, openMemoryOpsSettings, validateConfig } from "./config";

let statusBarItem: vscode.StatusBarItem;

export function activate(context: vscode.ExtensionContext): void {
  statusBarItem = vscode.window.createStatusBarItem(vscode.StatusBarAlignment.Left, 100);
  statusBarItem.command = "memoryops.testConnection";
  statusBarItem.text = "$(database) MemoryOps";
  statusBarItem.tooltip = "MemoryOps: Test connection";
  statusBarItem.show();
  context.subscriptions.push(statusBarItem);

  context.subscriptions.push(
    vscode.commands.registerCommand("memoryops.testConnection", testConnection),
    vscode.commands.registerCommand("memoryops.searchMemory", searchMemory),
    vscode.commands.registerCommand("memoryops.retrieveContextForCurrentFile", retrieveContextForCurrentFile),
    vscode.commands.registerCommand("memoryops.saveSelectionAsObservation", saveSelectionAsObservation),
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

  const results = await vscode.window.withProgress(
    {
      location: vscode.ProgressLocation.Notification,
      title: "Searching MemoryOps...",
      cancellable: false,
    },
    () => client.searchMemory(query.trim(), config.defaultTopK),
  );

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
  const repo = getWorkspaceRepoHint();
  const fileName = document?.fileName ?? "current editor";

  const query = [
    `Relevant MemoryOps context for ${fileName}`,
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
    () => client.retrieve(query, config.defaultTokenBudget),
  );

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

  const repo = getWorkspaceRepoHint();
  const sourceRef = editor.document.uri.scheme === "file" ? editor.document.fileName : editor.document.uri.toString();

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
      description: [result.memory_type, result.source, scoreLabel(result.score)].filter(Boolean).join(" · "),
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
  const markdown = [
    `# ${title}`,
    result.query_id ? `Query ID: \`${result.query_id}\`` : undefined,
    "",
    result.packed_context ?? result.context,
    ...(Array.isArray(result.memories) && result.memories.length > 0
      ? [
          "",
          "## Memories",
          ...result.memories.map((memory, index) => [
            `### ${index + 1}. ${memory.id ?? "Memory"}`,
            memory.score !== undefined ? `Score: ${memory.score}` : undefined,
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
    result.source ? `Source: ${result.source}` : undefined,
    Array.isArray(result.tags) && result.tags.length > 0 ? `Tags: ${result.tags.join(", ")}` : undefined,
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

function getWorkspaceRepoHint(): string | undefined {
  const folder = vscode.workspace.workspaceFolders?.[0];
  return folder?.name;
}

function firstLine(value: string): string {
  return value.split(/\r?\n/)[0] ?? value;
}

function truncate(value: string, maxLength: number): string {
  return value.length <= maxLength ? value : `${value.slice(0, maxLength - 1)}…`;
}

function scoreLabel(score: unknown): string | undefined {
  return typeof score === "number" ? `score ${score.toFixed(3)}` : undefined;
}
