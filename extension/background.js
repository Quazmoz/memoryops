/**
 * MemoryOps Chrome Extension — Background Service Worker
 *
 * IMPORTANT: chrome.runtime.onMessage listeners must return `true` synchronously
 * if the response will be sent asynchronously (i.e. after an `await`). Failing
 * to do so causes the message port to close before `sendResponse` is called,
 * silently dropping the response to the content script / popup.
 *
 * Pattern used here:
 *   1. The addListener callback is NOT async — it returns `true` synchronously.
 *   2. All async work is delegated to the named `handleMessage` function.
 *   3. `.catch()` ensures sendResponse is always called even on error.
 */

import { getSettings, saveSettings } from "./settings.js";

// ── Entry point ───────────────────────────────────────────────────────────────

chrome.runtime.onMessage.addListener((message, sender, sendResponse) => {
  // Kick off async work but return true synchronously so Chrome keeps the
  // message channel open until sendResponse is eventually called.
  handleMessage(message, sender)
    .then(sendResponse)
    .catch((err) => sendResponse({ error: err.message ?? String(err) }));

  return true; // CRITICAL — keeps the port open for the async sendResponse
});

// ── Message dispatcher ────────────────────────────────────────────────────────

/**
 * Route an incoming message to the appropriate handler and return a response
 * object.  All async logic lives here; the addListener callback above stays
 * synchronous so the port is never prematurely closed.
 *
 * @param {object} message  - The message sent by the content script or popup.
 * @param {chrome.runtime.MessageSender} sender - Sender metadata.
 * @returns {Promise<object>} Response payload to forward back via sendResponse.
 */
async function handleMessage(message, sender) {
  switch (message.type) {
    case "INGEST_OBSERVATION":
      return ingestObservation(message.payload);

    case "RETRIEVE_MEMORY":
      return retrieveMemory(message.query);

    case "GET_SETTINGS":
      return { settings: await getSettings() };

    case "SAVE_SETTINGS":
      await saveSettings(message.settings);
      return { success: true };

    case "LIST_SKILLS":
      return { skills: await listSkills() };

    case "SET_SKILL_ENABLED":
      return {
        skill: await setSkillEnabled(message.name, message.enabled),
      };

    default:
      throw new Error(`Unknown message type: ${message.type}`);
  }
}

// ── Handlers ──────────────────────────────────────────────────────────────────

/**
 * Ingest an observation into the MemoryOps API.
 *
 * @param {object} payload  - Observation payload (content, source_url, etc.)
 * @returns {Promise<object>} API response body.
 */
async function ingestObservation(payload) {
  const settings = await getConfiguredSettings();
  return memoryOpsRequest(settings, "/v1/ingest/observation", {
    method: "POST",
    body: payload,
  });
}

/**
 * Retrieve memories from the MemoryOps API using a free-text query.
 *
 * @param {string} query  - Natural language search query.
 * @returns {Promise<object>} API response body (includes `results` array).
 */
async function retrieveMemory(query) {
  const settings = await getConfiguredSettings();
  return memoryOpsRequest(settings, "/v1/retrieve", {
    method: "POST",
    body: { query, workspace_id: settings.workspaceId },
  });
}

async function listSkills() {
  const settings = await getConfiguredSettings();
  return memoryOpsRequest(settings, `/v1/workspaces/${settings.workspaceId}/skills`);
}

async function setSkillEnabled(name, enabled) {
  const settings = await getConfiguredSettings();
  return memoryOpsRequest(settings, `/v1/workspaces/${settings.workspaceId}/skills/${encodeURIComponent(name)}`, {
    method: "PATCH",
    body: { enabled },
  });
}

async function getConfiguredSettings() {
  const settings = await getSettings();
  const apiUrl = settings.apiUrl?.trim().replace(/\/+$/, "") ?? "";
  const apiKey = settings.apiKey?.trim() ?? "";
  const workspaceId = settings.workspaceId?.trim() ?? "";

  if (!apiUrl || !apiKey || !workspaceId) {
    throw new Error("MemoryOps is not configured. Open the extension popup to set up your API credentials.");
  }

  return { apiUrl, apiKey, workspaceId };
}

async function memoryOpsRequest(settings, path, init = {}) {
  const headers = {
    "x-api-key": settings.apiKey,
    "x-workspace-id": settings.workspaceId,
    ...(init.body ? { "Content-Type": "application/json" } : {}),
    ...(init.headers ?? {}),
  };

  const response = await fetch(`${settings.apiUrl}${path}`, {
    method: init.method ?? "GET",
    headers,
    body: init.body ? JSON.stringify(init.body) : undefined,
  });

  if (!response.ok) {
    const body = await response.text().catch(() => "(no body)");
    throw new Error(`MemoryOps API returned ${response.status}: ${body}`);
  }

  if (response.status === 204) {
    return null;
  }

  return response.json();
}
