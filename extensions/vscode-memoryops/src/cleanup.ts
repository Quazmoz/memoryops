import * as fs from "fs";
import * as os from "os";
import * as path from "path";

import { applyEdits, findNodeAtLocation, modify, parseTree } from "jsonc-parser";

const JSON_FORMATTING_OPTIONS = {
  insertSpaces: true,
  tabSize: 2,
  eol: "\n",
};

const SETTINGS_PREFIX = "memoryops.";

export interface CleanupManifest {
  version: 1;
  settingsFiles: string[];
  storageDirectories: string[];
}

export const CLEANUP_MANIFEST_PATH = path.join(os.homedir(), ".memoryops-vscode-cleanup.json");

export function readCleanupManifest(filePath = CLEANUP_MANIFEST_PATH): CleanupManifest | undefined {
  try {
    if (!fs.existsSync(filePath)) {
      return undefined;
    }

    const raw = JSON.parse(fs.readFileSync(filePath, "utf8")) as Partial<CleanupManifest>;
    return normalizeManifest(raw);
  } catch {
    return undefined;
  }
}

export function updateCleanupManifest(
  update: Partial<CleanupManifest>,
  filePath = CLEANUP_MANIFEST_PATH,
): CleanupManifest {
  const current = readCleanupManifest(filePath);
  const next = normalizeManifest({
    ...current,
    ...update,
    settingsFiles: [
      ...(current?.settingsFiles ?? []),
      ...(update.settingsFiles ?? []),
    ],
    storageDirectories: [
      ...(current?.storageDirectories ?? []),
      ...(update.storageDirectories ?? []),
    ],
  });

  fs.writeFileSync(filePath, `${JSON.stringify(next, null, 2)}\n`, "utf8");
  return next;
}

export function collectCandidateUserSettingsFiles(
  home = os.homedir(),
  platform = process.platform,
): string[] {
  const existingFiles: string[] = [];

  for (const userDir of getVsCodeUserDirs(home, platform)) {
    const directSettingsFile = path.join(userDir, "settings.json");
    if (fs.existsSync(directSettingsFile)) {
      existingFiles.push(directSettingsFile);
    }

    const profilesDir = path.join(userDir, "profiles");
    if (!fs.existsSync(profilesDir)) {
      continue;
    }

    for (const entry of fs.readdirSync(profilesDir, { withFileTypes: true })) {
      if (!entry.isDirectory()) {
        continue;
      }

      const profileSettingsFile = path.join(profilesDir, entry.name, "settings.json");
      if (fs.existsSync(profileSettingsFile)) {
        existingFiles.push(profileSettingsFile);
      }
    }
  }

  return dedupePaths(existingFiles);
}

export function cleanSettingsFile(filePath: string): boolean {
  if (!fs.existsSync(filePath)) {
    return false;
  }

  const targetPath = filePath.endsWith(".code-workspace") ? ["settings"] : [];
  const input = fs.readFileSync(filePath, "utf8");
  const output = removeMemoryOpsSettings(input, targetPath);
  if (!output.changed) {
    return false;
  }

  fs.writeFileSync(filePath, output.content, "utf8");
  return true;
}

export function removeMemoryOpsSettings(
  text: string,
  jsonPath: string[] = [],
): { changed: boolean; content: string } {
  let next = text;
  let changed = false;

  while (true) {
    const keys = getMemoryOpsKeysAtPath(next, jsonPath);
    if (keys.length === 0) {
      break;
    }

    changed = true;
    for (const key of keys) {
      const edits = modify(next, [...jsonPath, key], undefined, {
        formattingOptions: JSON_FORMATTING_OPTIONS,
      });
      next = applyEdits(next, edits);
    }
  }

  if (!changed) {
    return { changed: false, content: text };
  }

  if (jsonPath.length > 0 && isEmptyObjectAtPath(next, jsonPath)) {
    const edits = modify(next, jsonPath, undefined, {
      formattingOptions: JSON_FORMATTING_OPTIONS,
    });
    next = applyEdits(next, edits);
  }

  return { changed: true, content: next };
}

function getMemoryOpsKeysAtPath(text: string, jsonPath: string[]): string[] {
  const tree = parseTree(text);
  if (!tree) {
    return [];
  }

  const objectNode = jsonPath.length === 0 ? tree : findNodeAtLocation(tree, jsonPath);
  if (!objectNode || objectNode.type !== "object" || !objectNode.children) {
    return [];
  }

  return objectNode.children
    .map((propertyNode) => propertyNode.children?.[0]?.value)
    .filter((value): value is string => typeof value === "string" && value.startsWith(SETTINGS_PREFIX));
}

function isEmptyObjectAtPath(text: string, jsonPath: string[]): boolean {
  const tree = parseTree(text);
  if (!tree) {
    return false;
  }

  const objectNode = findNodeAtLocation(tree, jsonPath);
  return objectNode?.type === "object" && (objectNode.children?.length ?? 0) === 0;
}

function normalizeManifest(raw: Partial<CleanupManifest> | undefined): CleanupManifest {
  return {
    version: 1,
    settingsFiles: dedupePaths(raw?.settingsFiles ?? []),
    storageDirectories: dedupePaths(raw?.storageDirectories ?? []),
  };
}

function dedupePaths(paths: readonly string[]): string[] {
  const results: string[] = [];
  const seen = new Set<string>();

  for (const candidate of paths) {
    if (typeof candidate !== "string" || candidate.trim().length === 0) {
      continue;
    }

    const normalized = path.resolve(candidate);
    const key = process.platform === "win32" ? normalized.toLowerCase() : normalized;
    if (seen.has(key)) {
      continue;
    }

    seen.add(key);
    results.push(normalized);
  }

  return results;
}

function getVsCodeUserDirs(home: string, platform: string): string[] {
  if (platform === "win32") {
    const appData = process.env.APPDATA || path.join(home, "AppData", "Roaming");
    return [
      path.join(appData, "Code", "User"),
      path.join(appData, "Code - Insiders", "User"),
    ];
  }

  if (platform === "darwin") {
    return [
      path.join(home, "Library", "Application Support", "Code", "User"),
      path.join(home, "Library", "Application Support", "Code - Insiders", "User"),
    ];
  }

  return [
    path.join(home, ".config", "Code", "User"),
    path.join(home, ".config", "Code - Insiders", "User"),
  ];
}
