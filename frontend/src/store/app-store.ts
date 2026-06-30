import { create } from "zustand";
import { createJSONStorage, persist, type StateStorage } from "zustand/middleware";

export const APP_STORE_STORAGE_KEY = "memoryops-app-store";

export type AppStore = {
  workspaceId: string;
  apiKey: string;
  setWorkspace: (workspaceId: string, apiKey: string) => void;
  setWorkspaceId: (workspaceId: string) => void;
  setApiKey: (apiKey: string) => void;
  clearApiKey: () => void;
  clearWorkspace: () => void;
};

const initialWorkspaceId = (import.meta.env.VITE_MEMORYOPS_WORKSPACE_ID ?? "").trim();
const appStoreStorage = resolveAppStoreStorage();

export const useAppStore = create<AppStore>()(
  persist(
    (set) => ({
      // Dev fallback: use VITE_MEMORYOPS_WORKSPACE_ID when available (local `npm run dev`).
      // In containerised deployments the runtime /config.json (loaded in main.tsx)
      // will override this value before the app renders.
      workspaceId: initialWorkspaceId,
      apiKey: "",
      setWorkspace: (workspaceId, apiKey) =>
        set({ workspaceId: workspaceId.trim(), apiKey: apiKey.trim() }),
      setWorkspaceId: (workspaceId) => set({ workspaceId: workspaceId.trim() }),
      setApiKey: (apiKey) => set({ apiKey: apiKey.trim() }),
      clearApiKey: () => set({ apiKey: "" }),
      clearWorkspace: () => set({ workspaceId: "", apiKey: "" }),
    }),
    {
      name: APP_STORE_STORAGE_KEY,
      storage: createJSONStorage(() => appStoreStorage),
      partialize: (state) => ({
        workspaceId: state.workspaceId,
        apiKey: state.apiKey,
      }),
    },
  ),
);

export function clearStoredAppState(): void {
  appStoreStorage.removeItem(APP_STORE_STORAGE_KEY);
}

function resolveAppStoreStorage(): StateStorage {
  const fallbackStorage = createMemoryStorage();
  if (typeof window === "undefined") {
    return fallbackStorage;
  }

  try {
    const storage = window.localStorage;
    const probeKey = `${APP_STORE_STORAGE_KEY}:probe`;
    storage.setItem(probeKey, "1");
    storage.removeItem(probeKey);
    return storage;
  } catch {
    return fallbackStorage;
  }
}

function createMemoryStorage(): StateStorage {
  const values = new Map<string, string>();
  return {
    getItem: (name) => values.get(name) ?? null,
    setItem: (name, value) => values.set(name, value),
    removeItem: (name) => values.delete(name),
  };
}

/**
 * Load runtime workspace configuration from /config.json.
 *
 * This is called once from main.tsx before the React tree mounts so the store
 * is pre-populated.  In development (`npm run dev`) the file may not exist;
 * errors are silently swallowed and the Vite env-var fallback is used instead.
 */
export async function loadRuntimeConfig(): Promise<void> {
  try {
    const response = await fetch("/config.json", { cache: "no-store" });
    if (!response.ok) return;
    const cfg = (await response.json()) as { workspaceId?: unknown };
    const workspaceId = typeof cfg.workspaceId === "string" ? cfg.workspaceId.trim() : "";
    if (workspaceId.length > 0) {
      useAppStore.getState().setWorkspaceId(workspaceId);
    }
  } catch {
    // Silently ignore — /config.json may not be present in local dev mode.
  }
}
