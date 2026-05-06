/**
 * MemoryOps Extension — Settings helpers
 *
 * Thin wrapper around chrome.storage.sync so settings logic isn't
 * scattered across background.js and the popup.
 */

const SETTINGS_KEY = "memoryops_settings";

/**
 * @typedef {object} MemoryOpsSettings
 * @property {string} apiUrl      - Base URL of the MemoryOps API (e.g. "http://localhost:3000")
 * @property {string} apiKey      - API key (mops_…)
 * @property {string} workspaceId - UUID of the target workspace
 */

/**
 * Read settings from chrome.storage.sync.
 *
 * @returns {Promise<MemoryOpsSettings>}
 */
export async function getSettings() {
  return new Promise((resolve) => {
    chrome.storage.sync.get(SETTINGS_KEY, (result) => {
      resolve(result[SETTINGS_KEY] ?? { apiUrl: "", apiKey: "", workspaceId: "" });
    });
  });
}

/**
 * Persist settings to chrome.storage.sync.
 *
 * @param {MemoryOpsSettings} settings
 * @returns {Promise<void>}
 */
export async function saveSettings(settings) {
  return new Promise((resolve, reject) => {
    chrome.storage.sync.set({ [SETTINGS_KEY]: settings }, () => {
      if (chrome.runtime.lastError) {
        reject(new Error(chrome.runtime.lastError.message));
      } else {
        resolve();
      }
    });
  });
}
