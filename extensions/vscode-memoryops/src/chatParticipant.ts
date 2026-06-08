import * as vscode from "vscode";
import { MemoryOpsClient, MemorySearchResult } from "./client";
import { MemoryOpsConfig } from "./config";
import { errorMessage, firstLine, truncate } from "./markdown";
import { getWorkspaceRepoHint } from "./repo";

export const CHAT_PARTICIPANT_ID = "memoryops.chat";

export interface ChatClientContext {
  client: MemoryOpsClient;
  config: MemoryOpsConfig;
  missing: string[];
}

type GetClient = () => ChatClientContext;

interface MemoryChatResult extends vscode.ChatResult {
  metadata: { command: string };
}

/**
 * Registers the `@memoryops` Copilot Chat participant so users can query their
 * MemoryOps workspace directly from the chat panel.
 *
 * Supported slash commands:
 *   /search   — keyword/hybrid search over the workspace (default behaviour)
 *   /retrieve — packed retrieval context for the prompt (token-budgeted)
 */
export function registerChatParticipant(
  context: vscode.ExtensionContext,
  getClient: GetClient,
): void {
  // `vscode.chat` is only present when a chat-capable client (e.g. GitHub
  // Copilot Chat) is installed. Guard so activation never throws without it.
  if (!vscode.chat || typeof vscode.chat.createChatParticipant !== "function") {
    return;
  }

  const handler: vscode.ChatRequestHandler = async (
    request,
    _chatContext,
    stream,
    token,
  ): Promise<MemoryChatResult> => {
    const command = request.command ?? "search";
    const { client, config, missing } = getClient();

    if (missing.length > 0) {
      stream.markdown(
        `⚠️ MemoryOps is not configured yet (${missing.join(", ")}).\n\n`,
      );
      stream.button({
        command: "memoryops.openSettings",
        title: "Open MemoryOps Settings",
      });
      return { metadata: { command } };
    }

    const prompt = request.prompt.trim();
    if (!prompt) {
      stream.markdown(
        "Ask me about your MemoryOps workspace. For example:\n\n" +
          "- `@memoryops how do we handle auth tokens?`\n" +
          "- `@memoryops /retrieve deployment runbook`\n",
      );
      return { metadata: { command } };
    }

    try {
      const repo = await getWorkspaceRepoHint(vscode.window.activeTextEditor?.document);

      if (command === "retrieve") {
        await handleRetrieve(client, config, prompt, repo, stream);
      } else {
        await handleSearch(client, config, prompt, repo, stream, token);
      }
    } catch (error) {
      stream.markdown(`❌ MemoryOps request failed: ${errorMessage(error)}`);
    }

    return { metadata: { command } };
  };

  const participant = vscode.chat.createChatParticipant(CHAT_PARTICIPANT_ID, handler);
  participant.iconPath = vscode.Uri.joinPath(context.extensionUri, "media", "memoryops.png");
  participant.followupProvider = {
    provideFollowups(result: MemoryChatResult): vscode.ChatFollowup[] {
      // Offer the complementary command to whatever the user just ran.
      if (result.metadata.command === "retrieve") {
        return [{ prompt: "", command: "search", label: "Search these instead" }];
      }
      return [{ prompt: "", command: "retrieve", label: "Get packed context for this" }];
    },
  };

  context.subscriptions.push(participant);
}

async function handleSearch(
  client: MemoryOpsClient,
  config: MemoryOpsConfig,
  prompt: string,
  repo: string | undefined,
  stream: vscode.ChatResponseStream,
  token: vscode.CancellationToken,
): Promise<void> {
  stream.progress("Searching MemoryOps…");
  const results = await client.searchMemory(prompt, config.defaultTopK, {
    mode: config.defaultSearchMode,
    repo,
    includeWorkspacePool: config.includeWorkspacePool,
  });

  if (token.isCancellationRequested) {
    return;
  }

  if (results.length === 0) {
    stream.markdown(`No memories matched **${escapeMd(prompt)}**.`);
    return;
  }

  stream.markdown(`Found **${results.length}** ${results.length === 1 ? "memory" : "memories"} for **${escapeMd(prompt)}**:\n\n`);

  results.forEach((memory, index) => {
    renderMemory(stream, memory, index + 1);
  });
}

async function handleRetrieve(
  client: MemoryOpsClient,
  config: MemoryOpsConfig,
  prompt: string,
  repo: string | undefined,
  stream: vscode.ChatResponseStream,
): Promise<void> {
  stream.progress("Retrieving MemoryOps context…");
  const result = await client.retrieve(prompt, config.defaultTokenBudget, {
    mode: config.defaultSearchMode,
    repo,
    includeTrace: false,
    includeWorkspacePool: config.includeWorkspacePool,
  });

  const contextText = result.packed_context ?? result.context;
  if (!contextText?.trim()) {
    stream.markdown(`No context could be packed for **${escapeMd(prompt)}**.`);
    return;
  }

  if (typeof result.total_tokens === "number") {
    stream.markdown(`Packed context (~${result.total_tokens} tokens):\n\n`);
  }
  stream.markdown("```text\n" + contextText.trim() + "\n```\n");

  const memories = Array.isArray(result.memories) ? result.memories : [];
  if (memories.length > 0) {
    stream.markdown(`\n**Sources** (${memories.length}):\n\n`);
    memories.forEach((memory, index) => {
      const label = memory.content ? truncate(firstLine(memory.content), 100) : memory.id ?? `Memory ${index + 1}`;
      stream.markdown(`- ${escapeMd(label)}\n`);
    });
  }
}

function renderMemory(stream: vscode.ChatResponseStream, memory: MemorySearchResult, ordinal: number): void {
  const heading = memory.content ? truncate(firstLine(memory.content), 90) : memory.id ?? `Memory ${ordinal}`;
  const badges = [
    memory.memory_type,
    memory.scope_visibility,
    memory.pinned ? "pinned" : undefined,
    typeof memory.score === "number" ? `score ${memory.score.toFixed(2)}` : undefined,
  ]
    .filter(Boolean)
    .join(" · ");

  stream.markdown(`**${ordinal}. ${escapeMd(heading)}**`);
  if (badges) {
    stream.markdown(`  \n_${escapeMd(badges)}_`);
  }
  stream.markdown("\n\n");

  if (memory.content) {
    stream.markdown(truncate(memory.content, 600) + "\n\n");
  }

  if (memory.id) {
    stream.button({
      command: "memoryops.openMemory",
      title: "Open in editor",
      arguments: [memory.id],
    });
  }
}

function escapeMd(value: string): string {
  // Defang markdown control characters so memory content can't break layout.
  return value.replace(/([\\`*_{}\[\]()#+\-.!|>])/g, "\\$1");
}
