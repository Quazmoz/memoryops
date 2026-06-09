import * as vscode from "vscode";
import * as fs from "fs";
import * as path from "path";

import { MemoryOpsClient, Skill, SkillVersion, AgentSkill, AgentSkillVersion } from "./client";
import {
  errorMessage,
  formatSkillMarkdown,
  formatSkillTestMarkdown,
  formatSkillVersionsMarkdown,
  formatSkillInvokeMarkdown,
  formatSkillInvocationsMarkdown,
  formatAgentSkillVersionsMarkdown,
  truncate,
} from "./markdown";
import { SkillItem } from "./skillTree";

export interface SkillCommandDeps {
  getClient: () => { client: MemoryOpsClient; missing: string[] };
  promptForMissingConfig: (missing: string[]) => Promise<void>;
  openMarkdownDocument: (markdown: string) => Promise<void>;
  onSkillsChanged?: () => void;
}

const SKILL_NAME_PATTERN = /^[a-z][a-z0-9_]{0,63}$/;

export function registerSkillCommands(
  context: vscode.ExtensionContext,
  deps: SkillCommandDeps,
): void {
  context.subscriptions.push(
    vscode.commands.registerCommand("memoryops.skills.list", (item) => listSkillsCommand(deps, item)),
    vscode.commands.registerCommand("memoryops.skills.create", () => createSkillCommand(deps)),
    vscode.commands.registerCommand("memoryops.skills.toggleEnabled", (item) => toggleSkillEnabledCommand(deps, item)),
    vscode.commands.registerCommand("memoryops.skills.delete", (item) => deleteSkillCommand(deps, item)),
    vscode.commands.registerCommand("memoryops.skills.test", (item, version) => testSkillCommand(deps, item, version)),
    vscode.commands.registerCommand("memoryops.skills.viewHistory", (item) => viewSkillHistoryCommand(deps, item)),
    vscode.commands.registerCommand("memoryops.skills.rollback", (item, version) => rollbackSkillCommand(deps, item, version)),
    vscode.commands.registerCommand("memoryops.skills.invoke", (item, version) => invokeSkillCommand(deps, item, version)),
    vscode.commands.registerCommand("memoryops.skills.viewInvocations", (item) => viewSkillInvocationsCommand(deps, item)),
    vscode.commands.registerCommand("memoryops.agentSkills.viewHistory", (item) => viewAgentSkillHistoryCommand(deps, item)),
    vscode.commands.registerCommand("memoryops.agentSkills.rollback", (item, version) => rollbackAgentSkillCommand(deps, item, version)),
  );
}

async function withClient(
  deps: SkillCommandDeps,
): Promise<MemoryOpsClient | undefined> {
  const { client, missing } = deps.getClient();
  if (missing.length > 0) {
    await deps.promptForMissingConfig(missing);
    return undefined;
  }
  return client;
}

function isSkill(value: unknown): value is Skill {
  return typeof value === "object" && value !== null && !Array.isArray(value)
    && "name" in value && "endpoint_url" in value && "http_method" in value;
}

async function resolveSkillSelection(client: MemoryOpsClient, argument: unknown): Promise<Skill | undefined> {
  if (argument instanceof SkillItem) {
    return argument.skill;
  }
  if (isSkill(argument)) {
    return argument;
  }
  if (typeof argument === "string") {
    const skills = await client.listSkills();
    return skills.find((s) => s.name === argument);
  }
  if (typeof argument === "object" && argument !== null) {
    if ("name" in argument && typeof (argument as any).name === "string") {
      const name = (argument as any).name;
      const skills = await client.listSkills();
      return skills.find((s) => s.name === name);
    }
  }
  return pickSkill(client, "MemoryOps: Select Skill");
}

function extractVersion(argument: unknown): number | undefined {
  if (typeof argument === "object" && argument !== null) {
    if ("version" in argument && typeof (argument as any).version === "number") {
      return (argument as any).version;
    }
  }
  return undefined;
}

async function pickSkill(client: MemoryOpsClient, title: string): Promise<Skill | undefined> {
  let skills: Skill[];
  try {
    skills = await vscode.window.withProgress(
      { location: vscode.ProgressLocation.Notification, title: "Loading MemoryOps skills...", cancellable: false },
      () => client.listSkills(),
    );
  } catch (error) {
    void vscode.window.showErrorMessage(`Could not load skills: ${errorMessage(error)}`);
    return undefined;
  }
  if (skills.length === 0) {
    void vscode.window.showInformationMessage("No skills registered. Use 'MemoryOps: Create Skill' to add one.");
    return undefined;
  }
  const picked = await vscode.window.showQuickPick(
    skills.map((skill) => ({
      label: `${skill.enabled ? "$(check)" : "$(circle-slash)"} ${skill.name}`,
      description: `v${skill.version} · ${skill.http_method}`,
      detail: truncate(skill.description || skill.endpoint_url, 160),
      skill,
    })),
    { title, matchOnDescription: true, matchOnDetail: true },
  );
  return picked?.skill;
}

async function listSkillsCommand(deps: SkillCommandDeps, item?: unknown): Promise<void> {
  const client = await withClient(deps);
  if (!client) return;
  const skill = await resolveSkillSelection(client, item);
  if (!skill) return;
  await deps.openMarkdownDocument(formatSkillMarkdown(skill));
}

async function createSkillCommand(deps: SkillCommandDeps): Promise<void> {
  const client = await withClient(deps);
  if (!client) return;

  const name = await vscode.window.showInputBox({
    title: "Create skill — name",
    prompt: "Machine-safe identifier (lowercase letters, digits, underscores)",
    ignoreFocusOut: true,
    validateInput: (value) => SKILL_NAME_PATTERN.test(value.trim()) ? undefined : "Use lowercase letters, digits, and underscores (max 64 chars).",
  });
  if (!name) return;

  const description = await vscode.window.showInputBox({
    title: "Create skill — description",
    prompt: "What does this skill do? (1-500 chars)",
    ignoreFocusOut: true,
    validateInput: (v) => v.trim().length >= 1 && v.trim().length <= 500 ? undefined : "Enter 1-500 characters.",
  });
  if (!description) return;

  const endpointUrl = await vscode.window.showInputBox({
    title: "Create skill — endpoint URL",
    prompt: "HTTPS endpoint MemoryOps will call",
    value: "https://",
    ignoreFocusOut: true,
    validateInput: (v) => v.trim().startsWith("https://") ? undefined : "URL must start with https://.",
  });
  if (!endpointUrl) return;

  const method = await vscode.window.showQuickPick(["POST", "GET", "PUT"], {
    title: "Create skill — HTTP method",
    ignoreFocusOut: true,
  });
  if (!method) return;

  const authHeader = await vscode.window.showInputBox({
    title: "Create skill — auth header (optional)",
    prompt: "Header name to send when authenticating (e.g. Authorization). Leave blank for none.",
    ignoreFocusOut: true,
  });
  if (authHeader === undefined) return;

  let authSecret: string | undefined;
  if (authHeader.trim().length > 0) {
    authSecret = await vscode.window.showInputBox({
      title: "Create skill — auth secret",
      prompt: "Secret value sent with the auth header. Stored encrypted by the backend.",
      password: true,
      ignoreFocusOut: true,
    });
    if (authSecret === undefined) return;
  }

  const changeNote = await vscode.window.showInputBox({
    title: "Create skill — change note (optional)",
    prompt: "Short note describing this initial version.",
    ignoreFocusOut: true,
  });
  if (changeNote === undefined) return;

  try {
    const skill = await vscode.window.withProgress(
      { location: vscode.ProgressLocation.Notification, title: `Creating skill ${name}...`, cancellable: false },
      () => client.createSkill({
        name: name.trim(),
        description: description.trim(),
        endpoint_url: endpointUrl.trim(),
        http_method: method,
        auth_header: authHeader.trim() || undefined,
        auth_secret: authSecret && authSecret.length > 0 ? authSecret : undefined,
        change_note: changeNote.trim() || undefined,
      }),
    );
    void vscode.window.showInformationMessage(`Skill '${skill.name}' created (v${skill.version}).`);
    deps.onSkillsChanged?.();
    await deps.openMarkdownDocument(formatSkillMarkdown(skill));
  } catch (error) {
    void vscode.window.showErrorMessage(`Create skill failed: ${errorMessage(error)}`);
  }
}

async function toggleSkillEnabledCommand(deps: SkillCommandDeps, item?: unknown): Promise<void> {
  const client = await withClient(deps);
  if (!client) return;
  const skill = await resolveSkillSelection(client, item);
  if (!skill) return;
  try {
    const updated = await client.updateSkill(skill.name, {
      enabled: !skill.enabled,
      change_note: skill.enabled ? "disabled via VS Code" : "enabled via VS Code",
    });
    void vscode.window.showInformationMessage(`Skill '${updated.name}' is now ${updated.enabled ? "enabled" : "disabled"} (v${updated.version}).`);
    deps.onSkillsChanged?.();
  } catch (error) {
    void vscode.window.showErrorMessage(`Toggle skill failed: ${errorMessage(error)}`);
  }
}

async function deleteSkillCommand(deps: SkillCommandDeps, item?: unknown): Promise<void> {
  const client = await withClient(deps);
  if (!client) return;
  const skill = await resolveSkillSelection(client, item);
  if (!skill) return;
  const confirm = await vscode.window.showWarningMessage(
    `Delete skill '${skill.name}'? This cannot be undone.`,
    { modal: true },
    "Delete",
  );
  if (confirm !== "Delete") return;
  try {
    await client.deleteSkill(skill.name);
    void vscode.window.showInformationMessage(`Skill '${skill.name}' deleted.`);
    deps.onSkillsChanged?.();
  } catch (error) {
    void vscode.window.showErrorMessage(`Delete skill failed: ${errorMessage(error)}`);
  }
}

async function testSkillCommand(deps: SkillCommandDeps, item?: unknown, versionArg?: unknown): Promise<void> {
  const client = await withClient(deps);
  if (!client) return;
  const skill = await resolveSkillSelection(client, item);
  if (!skill) return;

  let version: number | undefined;
  if (typeof versionArg === "number") {
    version = versionArg;
  } else {
    version = extractVersion(item);
  }

  const defaultBody = JSON.stringify(skill.input_schema ?? {}, null, 2);
  const bodyText = await vscode.window.showInputBox({
    title: `Test skill ${skill.name}${version !== undefined ? ` (v${version})` : ""} — request body JSON`,
    prompt: "JSON body sent to the skill endpoint",
    value: defaultBody,
    ignoreFocusOut: true,
    validateInput: (v) => {
      try {
        JSON.parse(v || "{}");
        return undefined;
      } catch {
        return "Invalid JSON.";
      }
    },
  });
  if (bodyText === undefined) return;
  let body: unknown;
  try {
    body = JSON.parse(bodyText || "{}");
  } catch {
    void vscode.window.showErrorMessage("Invalid JSON body.");
    return;
  }
  try {
    const result = await vscode.window.withProgress(
      { location: vscode.ProgressLocation.Notification, title: `Testing skill ${skill.name}...`, cancellable: false },
      () => client.testSkill(skill.name, body, version),
    );
    await deps.openMarkdownDocument(formatSkillTestMarkdown(skill, result));
  } catch (error) {
    void vscode.window.showErrorMessage(`Test skill failed: ${errorMessage(error)}`);
  }
}

async function viewSkillHistoryCommand(deps: SkillCommandDeps, item?: unknown): Promise<void> {
  const client = await withClient(deps);
  if (!client) return;
  const skill = await resolveSkillSelection(client, item);
  if (!skill) return;
  try {
    const versions = await vscode.window.withProgress(
      { location: vscode.ProgressLocation.Notification, title: `Loading history for ${skill.name}...`, cancellable: false },
      () => client.listSkillVersions(skill.name),
    );
    await deps.openMarkdownDocument(formatSkillVersionsMarkdown(skill, versions));
  } catch (error) {
    void vscode.window.showErrorMessage(`Load skill history failed: ${errorMessage(error)}`);
  }
}

async function rollbackSkillCommand(deps: SkillCommandDeps, item?: unknown, versionArg?: unknown): Promise<void> {
  const client = await withClient(deps);
  if (!client) return;
  const skill = await resolveSkillSelection(client, item);
  if (!skill) return;

  let version: number | undefined;
  if (typeof versionArg === "number") {
    version = versionArg;
  } else {
    version = extractVersion(item);
  }

  if (version === undefined) {
    let versions: SkillVersion[];
    try {
      versions = await client.listSkillVersions(skill.name);
    } catch (error) {
      void vscode.window.showErrorMessage(`Load skill history failed: ${errorMessage(error)}`);
      return;
    }
    const candidates = versions.filter((v) => v.version !== skill.version);
    if (candidates.length === 0) {
      void vscode.window.showInformationMessage("No previous versions available to roll back to.");
      return;
    }
    const picked = await vscode.window.showQuickPick(
      candidates.map((v) => ({
        label: `v${v.version}`,
        description: v.change_note ?? "",
        detail: [v.created_at, v.created_by].filter(Boolean).join(" · "),
        version: v,
      })),
      { title: `Roll back ${skill.name} (current v${skill.version})` },
    );
    if (!picked) return;
    version = picked.version.version;
  }

  const note = await vscode.window.showInputBox({
    title: `Rollback ${skill.name} → v${version}`,
    prompt: "Change note (optional)",
    ignoreFocusOut: true,
  });
  if (note === undefined) return;
  const confirm = await vscode.window.showWarningMessage(
    `Roll back '${skill.name}' to v${version}? A new version will be created.`,
    { modal: true },
    "Roll back",
  );
  if (confirm !== "Roll back") return;
  try {
    const updated = await client.rollbackSkillVersion(skill.name, version, note.trim() || undefined);
    void vscode.window.showInformationMessage(`Rolled back '${updated.name}' to snapshot of v${version} (now v${updated.version}).`);
    deps.onSkillsChanged?.();
  } catch (error) {
    void vscode.window.showErrorMessage(`Rollback failed: ${errorMessage(error)}`);
  }
}

async function invokeSkillCommand(deps: SkillCommandDeps, item?: unknown, versionArg?: unknown): Promise<void> {
  const client = await withClient(deps);
  if (!client) return;
  const skill = await resolveSkillSelection(client, item);
  if (!skill) return;

  let version: number | undefined;
  if (typeof versionArg === "number") {
    version = versionArg;
  } else {
    version = extractVersion(item);
  }

  const defaultBody = JSON.stringify(skill.input_schema ?? {}, null, 2);
  const bodyText = await vscode.window.showInputBox({
    title: `Invoke skill ${skill.name}${version !== undefined ? ` (v${version})` : ""} — request body JSON`,
    prompt: "JSON body sent to the skill endpoint",
    value: defaultBody,
    ignoreFocusOut: true,
    validateInput: (v) => {
      try {
        JSON.parse(v || "{}");
        return undefined;
      } catch {
        return "Invalid JSON.";
      }
    },
  });
  if (bodyText === undefined) return;
  let body: unknown;
  try {
    body = JSON.parse(bodyText || "{}");
  } catch {
    void vscode.window.showErrorMessage("Invalid JSON body.");
    return;
  }
  try {
    const result = await vscode.window.withProgress(
      { location: vscode.ProgressLocation.Notification, title: `Invoking skill ${skill.name}...`, cancellable: false },
      () => client.invokeSkill(skill.name, body, version),
    );
    await deps.openMarkdownDocument(formatSkillInvokeMarkdown(skill, result, version));
  } catch (error) {
    void vscode.window.showErrorMessage(`Invoke skill failed: ${errorMessage(error)}`);
  }
}

async function viewSkillInvocationsCommand(deps: SkillCommandDeps, item?: unknown): Promise<void> {
  const client = await withClient(deps);
  if (!client) return;
  const skill = await resolveSkillSelection(client, item);
  if (!skill) return;
  try {
    const invocations = await vscode.window.withProgress(
      { location: vscode.ProgressLocation.Notification, title: `Loading invocations for ${skill.name}...`, cancellable: false },
      () => client.listSkillInvocations(skill.name),
    );
    await deps.openMarkdownDocument(formatSkillInvocationsMarkdown(skill.name, invocations));
  } catch (error) {
    void vscode.window.showErrorMessage(`Load skill invocations failed: ${errorMessage(error)}`);
  }
}

async function resolveAgentSkillSelection(client: MemoryOpsClient, argument: unknown): Promise<AgentSkill | undefined> {
  if (argument && typeof argument === "object" && "agentSkill" in argument) {
    return (argument as any).agentSkill;
  }
  if (isAgentSkill(argument)) {
    return argument;
  }
  return pickAgentSkill(client, "MemoryOps: Select Agent Skill");
}

function isAgentSkill(value: unknown): value is AgentSkill {
  return typeof value === "object" && value !== null && !Array.isArray(value)
    && "name" in value && "assistant" in value && "filename" in value;
}

async function pickAgentSkill(client: MemoryOpsClient, title: string): Promise<AgentSkill | undefined> {
  let skills: AgentSkill[];
  try {
    skills = await vscode.window.withProgress(
      { location: vscode.ProgressLocation.Notification, title: "Loading MemoryOps agent skills...", cancellable: false },
      () => client.listAgentSkills(),
    );
  } catch (error) {
    void vscode.window.showErrorMessage(`Could not load agent skills: ${errorMessage(error)}`);
    return undefined;
  }
  if (skills.length === 0) {
    void vscode.window.showInformationMessage("No agent skills registered.");
    return undefined;
  }
  const picked = await vscode.window.showQuickPick(
    skills.map((skill) => ({
      label: `${skill.assistant}/${skill.name}`,
      description: `v${skill.version}`,
      detail: skill.description,
      skill,
    })),
    { title, matchOnDescription: true, matchOnDetail: true },
  );
  return picked?.skill;
}

async function viewAgentSkillHistoryCommand(deps: SkillCommandDeps, item?: unknown): Promise<void> {
  const client = await withClient(deps);
  if (!client) return;
  const skill = await resolveAgentSkillSelection(client, item);
  if (!skill) return;
  try {
    const versions = await vscode.window.withProgress(
      { location: vscode.ProgressLocation.Notification, title: `Loading history for agent skill ${skill.assistant}/${skill.name}...`, cancellable: false },
      () => client.listAgentSkillVersions(skill.assistant, skill.name),
    );
    await deps.openMarkdownDocument(formatAgentSkillVersionsMarkdown(skill, versions));
  } catch (error) {
    void vscode.window.showErrorMessage(`Load agent skill history failed: ${errorMessage(error)}`);
  }
}

async function rollbackAgentSkillCommand(deps: SkillCommandDeps, item?: unknown, versionArg?: unknown): Promise<void> {
  const client = await withClient(deps);
  if (!client) return;
  const skill = await resolveAgentSkillSelection(client, item);
  if (!skill) return;

  let version: number | undefined;
  if (typeof versionArg === "number") {
    version = versionArg;
  } else {
    version = extractVersion(item);
  }

  if (version === undefined) {
    let versions: AgentSkillVersion[];
    try {
      versions = await client.listAgentSkillVersions(skill.assistant, skill.name);
    } catch (error) {
      void vscode.window.showErrorMessage(`Load agent skill history failed: ${errorMessage(error)}`);
      return;
    }
    const candidates = versions.filter((v) => v.version !== skill.version);
    if (candidates.length === 0) {
      void vscode.window.showInformationMessage("No previous versions available to roll back to.");
      return;
    }
    const picked = await vscode.window.showQuickPick(
      candidates.map((v) => ({
        label: `v${v.version}`,
        description: v.change_note ?? "",
        detail: [v.created_at, v.created_by].filter(Boolean).join(" · "),
        version: v,
      })),
      { title: `Roll back agent skill ${skill.assistant}/${skill.name} (current v${skill.version})` },
    );
    if (!picked) return;
    version = picked.version.version;
  }

  const note = await vscode.window.showInputBox({
    title: `Rollback agent skill ${skill.assistant}/${skill.name} → v${version}`,
    prompt: "Change note (optional)",
    ignoreFocusOut: true,
  });
  if (note === undefined) return;
  const confirm = await vscode.window.showWarningMessage(
    `Roll back agent skill '${skill.assistant}/${skill.name}' to v${version}? Local file will be updated and a new version created.`,
    { modal: true },
    "Roll back",
  );
  if (confirm !== "Roll back") return;
  try {
    const updated = await client.rollbackAgentSkillVersion(
      skill.assistant,
      skill.name,
      version,
      note.trim() || undefined,
    );

    // Update local file
    const folders = vscode.workspace.workspaceFolders;
    if (folders && folders.length > 0) {
      const workspaceRoot = folders[0].uri.fsPath;
      const dir = path.join(workspaceRoot, `.${skill.assistant}`, "skills");
      const localPath = path.join(dir, `${skill.name}.md`);
      fs.mkdirSync(dir, { recursive: true });
      fs.writeFileSync(localPath, updated.content, "utf8");
    }

    void vscode.window.showInformationMessage(`Rolled back agent skill '${updated.name}' to snapshot of v${version} (now v${updated.version}).`);
    deps.onSkillsChanged?.();
  } catch (error) {
    void vscode.window.showErrorMessage(`Rollback failed: ${errorMessage(error)}`);
  }
}
