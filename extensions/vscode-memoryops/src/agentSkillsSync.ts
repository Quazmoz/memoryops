import * as fs from "fs";
import * as path from "path";
import * as vscode from "vscode";
import { MemoryOpsClient, AgentSkillContent } from "./client";

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

export async function syncAgentSkills(client: MemoryOpsClient): Promise<void> {
  const folders = vscode.workspace.workspaceFolders;
  if (!folders || folders.length === 0) {
    return;
  }

  const folder = folders[0];
  const workspaceRoot = folder.uri.fsPath;

  await vscode.window.withProgress(
    {
      location: vscode.ProgressLocation.Notification,
      title: "Syncing MemoryOps agent skills...",
      cancellable: false,
    },
    async (progress) => {
      try {
        // 1. Fetch remote skills
        progress.report({ message: "Listing remote agent skills..." });
        const remoteSkillsSummary = await client.listAgentSkills();
        const remoteSkillsMap = new Map<string, AgentSkillContent>();

        for (const skill of remoteSkillsSummary) {
          const content = await client.getAgentSkill(skill.assistant, skill.name);
          remoteSkillsMap.set(`${skill.assistant}/${skill.name}`, content);
        }

        // 2. Scan local skills
        progress.report({ message: "Scanning local agent skills..." });
        const assistants = ["gemini", "claude"];
        const localSkillsMap = new Map<string, { content: string; parsed: ParsedAgentSkill }>();

        for (const assistant of assistants) {
          const dir = path.join(workspaceRoot, `.${assistant}`, "skills");
          if (!fs.existsSync(dir)) {
            continue;
          }
          const files = fs.readdirSync(dir);
          for (const file of files) {
            if (file.endsWith(".md")) {
              const name = path.basename(file, ".md");
              const filePath = path.join(dir, file);
              const content = fs.readFileSync(filePath, "utf8");
              const parsed = parseMarkdownMetadata(content, name);
              localSkillsMap.set(`${assistant}/${name}`, { content, parsed });
            }
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
            });
          } else {
            // Compare content
            const localNorm = localSkill.content.replace(/\r\n/g, "\n").trim();
            const remoteNorm = remoteSkill.content.replace(/\r\n/g, "\n").trim();

            if (localNorm !== remoteNorm) {
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
                });
              } else if (choice === "Keep Remote (Overwrite Local)") {
                progress.report({ message: `Downloading remote skill: ${key}...` });
                const dir = path.join(workspaceRoot, `.${assistant}`, "skills");
                fs.mkdirSync(dir, { recursive: true });
                fs.writeFileSync(path.join(dir, `${name}.md`), remoteSkill.content, "utf8");
              }
            }
          }
        }

        // Handle skills that exist only on remote
        for (const [key, remoteSkill] of remoteSkillsMap.entries()) {
          if (!localSkillsMap.has(key)) {
            const [assistant, name] = key.split("/");
            progress.report({ message: `Downloading new remote skill: ${key}...` });
            const dir = path.join(workspaceRoot, `.${assistant}`, "skills");
            fs.mkdirSync(dir, { recursive: true });
            fs.writeFileSync(path.join(dir, `${name}.md`), remoteSkill.content, "utf8");
          }
        }

        void vscode.window.showInformationMessage("MemoryOps agent skills sync complete!");
      } catch (err: any) {
        void vscode.window.showErrorMessage(`Agent skills sync failed: ${err.message}`);
      }
    }
  );
}
