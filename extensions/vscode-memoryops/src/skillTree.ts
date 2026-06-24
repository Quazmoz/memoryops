import * as vscode from "vscode";

import { MemoryOpsClient, Skill, AgentSkill } from "./client";

export type SkillTreeNode = CategoryItem | SkillItem | AgentSkillItem | MessageItem;

export class SkillTreeProvider implements vscode.TreeDataProvider<SkillTreeNode> {
  private readonly onDidChangeTreeDataEmitter = new vscode.EventEmitter<SkillTreeNode | undefined | null | void>();
  readonly onDidChangeTreeData = this.onDidChangeTreeDataEmitter.event;

  private skills: Skill[] = [];
  private agentSkills: AgentSkill[] = [];
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
      if (element instanceof CategoryItem) {
        return element.children;
      }
      return [];
    }

    if (!this.isLoaded) {
      const { client, missing } = this.getClient();
      if (missing.length > 0) {
        this.message = "Configure settings to load skills.";
        this.skills = [];
        this.agentSkills = [];
        this.isLoaded = true;
        return [new MessageItem(this.message)];
      }

      if (!client) {
        this.message = "Client not configured.";
        this.skills = [];
        this.agentSkills = [];
        this.isLoaded = true;
        return [new MessageItem(this.message)];
      }

      try {
        const [skills, agentSkills] = await Promise.all([
          client.listSkills(),
          client.listAgentSkills(),
        ]);
        this.skills = skills;
        this.agentSkills = agentSkills;
        this.isLoaded = true;
      } catch (error) {
        this.message = `Failed to load skills: ${error instanceof Error ? error.message : String(error)}`;
        this.skills = [];
        this.agentSkills = [];
        this.isLoaded = true;
        return [new MessageItem(this.message)];
      }
    }

    const folders = vscode.workspace.workspaceFolders;
    const folder = folders && folders.length > 0 ? folders[0] : undefined;

    const httpToolItems = this.skills.map((skill) => new SkillItem(skill));
    const geminiSkillItems: (AgentSkillItem | MessageItem)[] = [];
    const claudeSkillItems: (AgentSkillItem | MessageItem)[] = [];

    for (const skill of this.agentSkills) {
      let localUri: vscode.Uri | undefined;
      if (folder) {
        const candidateUri = vscode.Uri.joinPath(folder.uri, `.${skill.assistant}`, "skills", `${skill.name}.md`);
        try {
          await vscode.workspace.fs.stat(candidateUri);
          localUri = candidateUri;
        } catch {
          // file doesn't exist
        }
      }
      const item = new AgentSkillItem(skill, localUri);
      if (skill.assistant === "gemini") {
        geminiSkillItems.push(item);
      } else if (skill.assistant === "claude") {
        claudeSkillItems.push(item);
      }
    }

    if (geminiSkillItems.length === 0) {
      geminiSkillItems.push(new MessageItem("No Gemini agent skills."));
    }
    if (claudeSkillItems.length === 0) {
      claudeSkillItems.push(new MessageItem("No Claude agent skills."));
    }

    const rootNodes: SkillTreeNode[] = [];

    if (httpToolItems.length > 0) {
      rootNodes.push(new CategoryItem("Workspace HTTP Tools", "http_tools", httpToolItems));
    } else {
      rootNodes.push(new CategoryItem("Workspace HTTP Tools", "http_tools", [new MessageItem("No HTTP tools registered.")]));
    }

    rootNodes.push(new CategoryItem("Gemini Agent Skills", "agent_skills_gemini", geminiSkillItems));
    rootNodes.push(new CategoryItem("Claude Agent Skills", "agent_skills_claude", claudeSkillItems));

    return rootNodes;
  }
}

export class CategoryItem extends vscode.TreeItem {
  constructor(
    label: string,
    readonly categoryType: "http_tools" | "agent_skills_gemini" | "agent_skills_claude",
    readonly children: (SkillItem | AgentSkillItem | MessageItem)[],
  ) {
    super(label, vscode.TreeItemCollapsibleState.Expanded);
    this.contextValue = `memoryops.category.${categoryType}`;
    this.iconPath = new vscode.ThemeIcon(
      categoryType === "http_tools" ? "globe" : "hubot"
    );
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

export class AgentSkillItem extends vscode.TreeItem {
  constructor(readonly agentSkill: AgentSkill, readonly localUri?: vscode.Uri) {
    super(`${agentSkill.assistant}/${agentSkill.name}`, vscode.TreeItemCollapsibleState.None);

    this.id = `memoryops.agentSkill.${agentSkill.assistant}.${agentSkill.name}`;
    this.description = `v${agentSkill.version}`;
    this.tooltip = [
      `Name: ${agentSkill.name}`,
      `Assistant: ${agentSkill.assistant}`,
      `Title: ${agentSkill.title}`,
      `Description: ${agentSkill.description}`,
      localUri ? `Local Uri: ${localUri.toString()}` : "Not found locally",
    ].join("\n");

    this.contextValue = "memoryops.agentSkill";
    this.iconPath = new vscode.ThemeIcon("symbol-class");

    if (localUri) {
      this.command = {
        command: "vscode.open",
        title: "Open Local File",
        arguments: [localUri],
      };
    }
  }
}

export class MessageItem extends vscode.TreeItem {
  constructor(message: string) {
    super(message, vscode.TreeItemCollapsibleState.None);
    this.iconPath = new vscode.ThemeIcon("info");
  }
}
