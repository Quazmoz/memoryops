import * as vscode from "vscode";
import { MemoryOpsClient, AgentSkillContent } from "./client";
import { errorMessage } from "./markdown";

interface ParsedAgentSkill {
  title: string;
  description: string;
  instructions: string;
}

function parseMarkdownMetadata(content: string, fallbackName: string): ParsedAgentSkill {
  const normalized = content.replace(/\r\n/g, "\n");
  const lines = normalized.split("\n");
  let title = fallbackName;
  let description = "";
  let bodyStart = 0;

  let index = 0;
  while (index < lines.length && lines[index].trim() === "") {
    index++;
  }

  if (index < lines.length) {
    const line = lines[index].trim();
    if (line.startsWith("# Skill:")) {
      title = line.substring("# Skill:".length).trim();
      bodyStart = index + 1;
    } else if (line.startsWith("# ")) {
      title = line.substring("# ".length).trim();
      bodyStart = index + 1;
    }
  }

  let descriptionIndex = bodyStart;
  while (descriptionIndex < lines.length && lines[descriptionIndex].trim() === "") {
    descriptionIndex++;
  }

  if (descriptionIndex < lines.length) {
    const line = lines[descriptionIndex].trim();
    if (line.startsWith("**Description:**")) {
      description = line.substring("**Description:**".length).trim();
      bodyStart = descriptionIndex + 1;
    }
  }

  if (!description) {
    description = `Instructions on how to configure and run the ${title} agent skill.`;
  }

  const instructions = lines
    .slice(bodyStart)
    .join("\n")
    .trim();

  return { title, description, instructions };
}

function composeAgentSkillMarkdown(title: string, description: string, instructions: string): string {
  const trimmedInstructions = instructions.trim();
  if (trimmedInstructions === "") {
    return `# Skill: ${title}\n\n**Description:** ${description}\n`;
  } else {
    return `# Skill: ${title}\n\n**Description:** ${description}\n\n${trimmedInstructions}\n`;
  }
}

export async function syncAgentSkills(
  client: MemoryOpsClient,
  options: { interactive?: boolean } = {}
): Promise<void> {
  const interactive = options.interactive ?? false;
  const folders = vscode.workspace.workspaceFolders;
  if (!folders || folders.length === 0) {
    return;
  }

  const folder = folders[0];

  await vscode.window.withProgress(
    {
      location: interactive ? vscode.ProgressLocation.Notification : vscode.ProgressLocation.Window,
      title: "Syncing MemoryOps agent skills...",
      cancellable: false,
    },
    async (progress) => {
      try {
        let conflictDetected = false;

        // 1. Fetch remote skills in parallel
        progress.report({ message: "Listing remote agent skills..." });
        const remoteSkillsSummary = await client.listAgentSkills();
        const remoteSkillsMap = new Map<string, AgentSkillContent>();

        await Promise.all(
          remoteSkillsSummary.map(async (skill) => {
            const content = await client.getAgentSkill(skill.assistant, skill.name);
            remoteSkillsMap.set(`${skill.assistant}/${skill.name}`, content);
          })
        );

        // 2. Scan local skills using vscode.workspace.fs
        progress.report({ message: "Scanning local agent skills..." });
        const assistants = ["gemini", "claude"];
        const localSkillsMap = new Map<string, { content: string; parsed: ParsedAgentSkill }>();

        for (const assistant of assistants) {
          const dirUri = vscode.Uri.joinPath(folder.uri, `.${assistant}`, "skills");
          try {
            const entries = await vscode.workspace.fs.readDirectory(dirUri);
            for (const [nameWithExt, type] of entries) {
              if (type === vscode.FileType.File && nameWithExt.endsWith(".md")) {
                const name = nameWithExt.substring(0, nameWithExt.length - 3);
                const fileUri = vscode.Uri.joinPath(dirUri, nameWithExt);
                const rawContent = await vscode.workspace.fs.readFile(fileUri);
                const content = Buffer.from(rawContent).toString("utf8");
                const parsed = parseMarkdownMetadata(content, name);
                localSkillsMap.set(`${assistant}/${name}`, { content, parsed });
              }
            }
          } catch {
            // Directory doesn't exist, ignore
          }
        }

        // 3. Sync
        // Handle skills that exist locally
        for (const [key, localSkill] of localSkillsMap.entries()) {
          const [assistant, name] = key.split("/");
          const remoteSkill = remoteSkillsMap.get(key);

          if (!remoteSkill) {
            // Upload to server
            progress.report({ message: `Uploading local skill: ${key}...` });
            await client.createAgentSkill({
              assistant,
              name,
              title: localSkill.parsed.title,
              description: localSkill.parsed.description,
              instructions: localSkill.parsed.instructions,
              change_note: "Synced via VS Code (initial upload)",
            });
          } else {
            // Compare content using normalized markdown representation
            const localNormalized = composeAgentSkillMarkdown(
              localSkill.parsed.title,
              localSkill.parsed.description,
              localSkill.parsed.instructions
            ).replace(/\r\n/g, "\n").trim();
            const remoteNorm = remoteSkill.content.replace(/\r\n/g, "\n").trim();

            if (localNormalized !== remoteNorm) {
              if (!interactive) {
                conflictDetected = true;
                continue;
              }

              // Conflict! Ask user
              const choice = await vscode.window.showWarningMessage(
                `Agent skill '${key}' has conflicting changes. Which version would you like to keep?`,
                "Keep Local (Upload)",
                "Keep Remote (Overwrite Local)",
                "Skip"
              );

              if (choice === "Keep Local (Upload)") {
                progress.report({ message: `Updating remote skill: ${key}...` });
                await client.updateAgentSkill(assistant, name, {
                  title: localSkill.parsed.title,
                  description: localSkill.parsed.description,
                  instructions: localSkill.parsed.instructions,
                  change_note: "Synced via VS Code (manual upload)",
                });
              } else if (choice === "Keep Remote (Overwrite Local)") {
                progress.report({ message: `Downloading remote skill: ${key}...` });
                const dirUri = vscode.Uri.joinPath(folder.uri, `.${assistant}`, "skills");
                const fileUri = vscode.Uri.joinPath(dirUri, `${name}.md`);
                await vscode.workspace.fs.createDirectory(dirUri);
                await vscode.workspace.fs.writeFile(fileUri, Buffer.from(remoteSkill.content, "utf8"));
              }
            }
          }
        }

        // Handle skills that exist only on remote
        for (const [key, remoteSkill] of remoteSkillsMap.entries()) {
          if (!localSkillsMap.has(key)) {
            const [assistant, name] = key.split("/");
            progress.report({ message: `Downloading new remote skill: ${key}...` });
            const dirUri = vscode.Uri.joinPath(folder.uri, `.${assistant}`, "skills");
            const fileUri = vscode.Uri.joinPath(dirUri, `${name}.md`);
            await vscode.workspace.fs.createDirectory(dirUri);
            await vscode.workspace.fs.writeFile(fileUri, Buffer.from(remoteSkill.content, "utf8"));
          }
        }

        if (conflictDetected) {
          void vscode.window
            .showWarningMessage(
              "MemoryOps: Some agent skills have conflicting changes.",
              "Resolve Sync Conflicts"
            )
            .then((action) => {
              if (action === "Resolve Sync Conflicts") {
                void vscode.commands.executeCommand("memoryops.skills.syncAgentSkills");
              }
            });
        } else if (interactive) {
          void vscode.window.showInformationMessage("MemoryOps agent skills sync complete!");
        }
      } catch (error) {
        void vscode.window.showErrorMessage(`Agent skills sync failed: ${errorMessage(error)}`);
      }
    }
  );
}
