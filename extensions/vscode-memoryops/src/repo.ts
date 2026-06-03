import * as path from "path";
import * as vscode from "vscode";

interface GitExtension {
  getAPI(version: 1): GitApi;
}

interface GitApi {
  repositories: GitRepository[];
}

interface GitRepository {
  rootUri: vscode.Uri;
  state: {
    remotes: GitRemote[];
  };
}

interface GitRemote {
  name: string;
  fetchUrl?: string;
  pushUrl?: string;
}

// Cache for repo hint to avoid redundant Git extension activation
const REPO_HINT_TTL_MS = 30_000;
let repoHintCache: { value: string | undefined; expiresAt: number } | undefined;

// Invalidate cache when workspace folders change
vscode.workspace.onDidChangeWorkspaceFolders(() => {
  repoHintCache = undefined;
});

export async function getWorkspaceRepoHint(document?: vscode.TextDocument): Promise<string | undefined> {
  if (repoHintCache && Date.now() < repoHintCache.expiresAt) {
    return repoHintCache.value;
  }

  const value = (await getGitRemoteRepoHint(document)) ?? getWorkspaceFolderName(document);
  repoHintCache = { value, expiresAt: Date.now() + REPO_HINT_TTL_MS };
  return value;
}

export function getSourceRef(editor: vscode.TextEditor): string {
  if (editor.document.uri.scheme !== "file") {
    return editor.document.uri.toString();
  }

  const relativePath = getRelativeFileName(editor.document);
  if (editor.selection.isEmpty) {
    return relativePath;
  }

  const startLine = editor.selection.start.line + 1;
  const endLine = editor.selection.end.line + 1;
  return startLine === endLine ? `${relativePath}#L${startLine}` : `${relativePath}#L${startLine}-L${endLine}`;
}

export function getRelativeFileName(document: vscode.TextDocument): string {
  return document.uri.scheme === "file"
    ? vscode.workspace.asRelativePath(document.uri, false).replace(/\\/g, "/")
    : document.uri.toString();
}

async function getGitRemoteRepoHint(document?: vscode.TextDocument): Promise<string | undefined> {
  const extension = vscode.extensions.getExtension<GitExtension>("vscode.git");
  if (!extension) {
    return undefined;
  }

  try {
    const git = extension.isActive ? extension.exports : await extension.activate();
    const api = git.getAPI(1);
    const repository = findRepository(api.repositories, document);
    const remote = repository?.state.remotes.find((candidate) => candidate.name === "origin")
      ?? repository?.state.remotes[0];
    return normalizeRemoteUrl(remote?.fetchUrl ?? remote?.pushUrl);
  } catch {
    return undefined;
  }
}

function findRepository(repositories: GitRepository[], document?: vscode.TextDocument): GitRepository | undefined {
  if (repositories.length === 0) {
    return undefined;
  }

  const folder = document ? vscode.workspace.getWorkspaceFolder(document.uri) : vscode.workspace.workspaceFolders?.[0];
  if (!folder) {
    return repositories[0];
  }

  return repositories.find((repository) => isEqualOrParent(folder.uri.fsPath, repository.rootUri.fsPath)) ?? repositories[0];
}

function isEqualOrParent(childPath: string, parentPath: string): boolean {
  const normalizedChild = normalizeFsPath(childPath);
  const normalizedParent = normalizeFsPath(parentPath);
  return normalizedChild === normalizedParent || normalizedChild.startsWith(`${normalizedParent}/`);
}

function normalizeFsPath(value: string): string {
  return path.resolve(value).replace(/\\/g, "/").toLowerCase();
}

function normalizeRemoteUrl(value: string | undefined): string | undefined {
  if (!value?.trim()) {
    return undefined;
  }

  const remote = value.trim();
  const scpLike = /^git@[^:]+:(?<owner>[^/]+)\/(?<repo>.+)$/.exec(remote);
  if (scpLike?.groups) {
    return stripGitSuffix(`${scpLike.groups.owner}/${scpLike.groups.repo}`);
  }

  try {
    const parsed = new URL(remote);
    const parts = parsed.pathname.split("/").filter(Boolean);
    if (parts.length >= 2) {
      return stripGitSuffix(`${parts[parts.length - 2]}/${parts[parts.length - 1]}`);
    }
  } catch {
    return stripGitSuffix(remote);
  }

  return stripGitSuffix(remote);
}

function stripGitSuffix(value: string): string {
  return value.replace(/\.git$/i, "");
}

function getWorkspaceFolderName(document?: vscode.TextDocument): string | undefined {
  const folder = document ? vscode.workspace.getWorkspaceFolder(document.uri) : vscode.workspace.workspaceFolders?.[0];
  return folder?.name;
}