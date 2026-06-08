import * as fs from "fs";

import {
  CLEANUP_MANIFEST_PATH,
  cleanSettingsFile,
  collectCandidateUserSettingsFiles,
  readCleanupManifest,
} from "./cleanup";

export interface CleanupSummary {
  cleanedSettingsFiles: string[];
  removedStorageDirectories: string[];
  errors: string[];
}

export function runUninstallCleanup(): CleanupSummary {
  const manifest = readCleanupManifest();
  const summary: CleanupSummary = {
    cleanedSettingsFiles: [],
    removedStorageDirectories: [],
    errors: [],
  };

  const settingsFiles = new Set([
    ...collectCandidateUserSettingsFiles(),
    ...(manifest?.settingsFiles ?? []),
  ]);

  for (const filePath of settingsFiles) {
    try {
      if (cleanSettingsFile(filePath)) {
        summary.cleanedSettingsFiles.push(filePath);
      }
    } catch (error) {
      summary.errors.push(`Failed to clean ${filePath}: ${toErrorMessage(error)}`);
    }
  }

  for (const directory of manifest?.storageDirectories ?? []) {
    try {
      if (!fs.existsSync(directory)) {
        continue;
      }

      fs.rmSync(directory, { recursive: true, force: true });
      summary.removedStorageDirectories.push(directory);
    } catch (error) {
      summary.errors.push(`Failed to remove ${directory}: ${toErrorMessage(error)}`);
    }
  }

  try {
    if (fs.existsSync(CLEANUP_MANIFEST_PATH)) {
      fs.rmSync(CLEANUP_MANIFEST_PATH, { force: true });
    }
  } catch (error) {
    summary.errors.push(`Failed to remove cleanup manifest: ${toErrorMessage(error)}`);
  }

  return summary;
}

function toErrorMessage(error: unknown): string {
  if (error instanceof Error) {
    return error.message;
  }
  return String(error);
}

if (require.main === module) {
  const summary = runUninstallCleanup();

  if (summary.cleanedSettingsFiles.length > 0) {
    console.log(`MemoryOps cleaned ${summary.cleanedSettingsFiles.length} settings file(s).`);
  }
  if (summary.removedStorageDirectories.length > 0) {
    console.log(`MemoryOps removed ${summary.removedStorageDirectories.length} storage director${summary.removedStorageDirectories.length === 1 ? "y" : "ies"}.`);
  }
  if (summary.errors.length > 0) {
    console.error(summary.errors.join("\n"));
  }
}
