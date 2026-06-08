import * as vscode from "vscode";
import { MemoryOpsClient, Skill } from "./client";

export type SkillTreeNode = SkillItem | MessageItem;

export class SkillTreeProvider implements vscode.TreeDataProvider<SkillTreeNode> {
  private readonly onDidChangeTreeDataEmitter = new vscode.EventEmitter<SkillTreeNode | undefined | null | void>();
  readonly onDidChangeTreeData = this.onDidChangeTreeDataEmitter.event;

  private skills: Skill[] = [];
  private message = "Loading MemoryOps skills...";
  private isLoaded = false;

  constructor(
    private readonly getClient: () => { client: MemoryOpsClient | undefined; missing: string[] }
  ) {}

  refresh(): void {
    this.isLoaded = false;
    this.onDidChangeTreeDataEmitter.fire();
  }

  getTreeItem(element: SkillTreeNode): vscode.TreeItem {
    return element;
  }

  async getChildren(element?: SkillTreeNode): Promise<SkillTreeNode[]> {
    if (element) {
      return [];
    }

    if (!this.isLoaded) {
      const { client, missing } = this.getClient();
      if (missing.length > 0) {
        this.message = "Configure settings to load skills.";
        this.skills = [];
        this.isLoaded = true;
        return [new MessageItem(this.message)];
      }

      if (!client) {
        this.message = "Client not configured.";
        this.skills = [];
        this.isLoaded = true;
        return [new MessageItem(this.message)];
      }

      try {
        this.skills = await client.listSkills();
        this.isLoaded = true;
      } catch (error) {
        this.message = `Failed to load skills: ${error instanceof Error ? error.message : String(error)}`;
        this.skills = [];
        this.isLoaded = true;
        return [new MessageItem(this.message)];
      }
    }

    if (this.skills.length === 0) {
      return [new MessageItem("No skills registered.")];
    }

    return this.skills.map((skill) => new SkillItem(skill));
  }
}

export class SkillItem extends vscode.TreeItem {
  constructor(readonly skill: Skill) {
    super(skill.name, vscode.TreeItemCollapsibleState.None);

    this.id = `memoryops.skill.${skill.name}`;
    this.description = `v${skill.version} · ${skill.http_method}`;
    this.tooltip = [
      `Name: ${skill.name}`,
      `Description: ${skill.description}`,
      `URL: ${skill.endpoint_url}`,
      `Method: ${skill.http_method}`,
      `Status: ${skill.enabled ? "Enabled" : "Disabled"}`,
      `Visibility: ${skill.scope_visibility}`,
    ].join("\n");

    this.contextValue = [
      "memoryops.skill",
      skill.enabled ? "enabled" : "disabled",
    ].join(".");

    this.iconPath = new vscode.ThemeIcon(
      skill.enabled ? "symbol-method" : "circle-slash"
    );

    this.command = {
      command: "memoryops.skills.list",
      title: "Show Skill Details",
      arguments: [this],
    };
  }
}

export class MessageItem extends vscode.TreeItem {
  constructor(message: string) {
    super(message, vscode.TreeItemCollapsibleState.None);
    this.iconPath = new vscode.ThemeIcon("info");
  }
}
