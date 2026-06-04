import * as vscode from "vscode";

import { MemoryOpsClient, Skill, SkillVersion } from "./client";
import {
  errorMessage,
  formatSkillMarkdown,
  formatSkillTestMarkdown,
  formatSkillVersionsMarkdown,
  truncate,
} from "./markdown";

export interface SkillCommandDeps {
  getClient: () => { client: MemoryOpsClient; missing: string[] };
  promptForMissingConfig: (missing: string[]) => Promise<void>;
  openMarkdownDocument: (markdown: string) => Promise<void>;
}

const SKILL_NAME_PATTERN = /^[a-z][a-z0-9_]{0,63}$/;

export function registerSkillCommands(
  context: vscode.ExtensionContext,
  deps: SkillCommandDeps,
): void {
  context.subscriptions.push(
    vscode.commands.registerCommand("memoryops.skills.list", () => listSkillsCommand(deps)),
    vscode.commands.registerCommand("memoryops.skills.create", () => createSkillCommand(deps)),
    vscode.commands.registerCommand("memoryops.skills.toggleEnabled", () => toggleSkillEnabledCommand(deps)),
    vscode.commands.registerCommand("memoryops.skills.delete", () => deleteSkillCommand(deps)),
    vscode.commands.registerCommand("memoryops.skills.test", () => testSkillCommand(deps)),
    vscode.commands.registerCommand("memoryops.skills.viewHistory", () => viewSkillHistoryCommand(deps)),
    vscode.commands.registerCommand("memoryops.skills.rollback", () => rollbackSkillCommand(deps)),
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

async function listSkillsCommand(deps: SkillCommandDeps): Promise<void> {
  const client = await withClient(deps);
  if (!client) return;
  const skill = await pickSkill(client, "MemoryOps: Skills");
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
    await deps.openMarkdownDocument(formatSkillMarkdown(skill));
  } catch (error) {
    void vscode.window.showErrorMessage(`Create skill failed: ${errorMessage(error)}`);
  }
}

async function toggleSkillEnabledCommand(deps: SkillCommandDeps): Promise<void> {
  const client = await withClient(deps);
  if (!client) return;
  const skill = await pickSkill(client, "MemoryOps: Toggle Skill Enabled");
  if (!skill) return;
  try {
    const updated = await client.updateSkill(skill.name, {
      enabled: !skill.enabled,
      change_note: skill.enabled ? "disabled via VS Code" : "enabled via VS Code",
    });
    void vscode.window.showInformationMessage(`Skill '${updated.name}' is now ${updated.enabled ? "enabled" : "disabled"} (v${updated.version}).`);
  } catch (error) {
    void vscode.window.showErrorMessage(`Toggle skill failed: ${errorMessage(error)}`);
  }
}

async function deleteSkillCommand(deps: SkillCommandDeps): Promise<void> {
  const client = await withClient(deps);
  if (!client) return;
  const skill = await pickSkill(client, "MemoryOps: Delete Skill");
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
  } catch (error) {
    void vscode.window.showErrorMessage(`Delete skill failed: ${errorMessage(error)}`);
  }
}

async function testSkillCommand(deps: SkillCommandDeps): Promise<void> {
  const client = await withClient(deps);
  if (!client) return;
  const skill = await pickSkill(client, "MemoryOps: Test Skill");
  if (!skill) return;

  const defaultBody = JSON.stringify(skill.input_schema ?? {}, null, 2);
  const bodyText = await vscode.window.showInputBox({
    title: `Test skill ${skill.name} — request body JSON`,
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
      () => client.testSkill(skill.name, body),
    );
    await deps.openMarkdownDocument(formatSkillTestMarkdown(skill, result));
  } catch (error) {
    void vscode.window.showErrorMessage(`Test skill failed: ${errorMessage(error)}`);
  }
}

async function viewSkillHistoryCommand(deps: SkillCommandDeps): Promise<void> {
  const client = await withClient(deps);
  if (!client) return;
  const skill = await pickSkill(client, "MemoryOps: View Skill History");
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

async function rollbackSkillCommand(deps: SkillCommandDeps): Promise<void> {
  const client = await withClient(deps);
  if (!client) return;
  const skill = await pickSkill(client, "MemoryOps: Roll Back Skill");
  if (!skill) return;
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
  const note = await vscode.window.showInputBox({
    title: `Rollback ${skill.name} → v${picked.version.version}`,
    prompt: "Change note (optional)",
    ignoreFocusOut: true,
  });
  if (note === undefined) return;
  const confirm = await vscode.window.showWarningMessage(
    `Roll back '${skill.name}' to v${picked.version.version}? A new version will be created.`,
    { modal: true },
    "Roll back",
  );
  if (confirm !== "Roll back") return;
  try {
    const updated = await client.rollbackSkillVersion(skill.name, picked.version.version, note.trim() || undefined);
    void vscode.window.showInformationMessage(`Rolled back '${updated.name}' to snapshot of v${picked.version.version} (now v${updated.version}).`);
  } catch (error) {
    void vscode.window.showErrorMessage(`Rollback failed: ${errorMessage(error)}`);
  }
}
