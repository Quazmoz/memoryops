import { create } from "zustand";

type AppStore = {
  workspaceId: string;
  apiKey: string;
  setWorkspace: (workspaceId: string, apiKey: string) => void;
  setWorkspaceId: (workspaceId: string) => void;
  setApiKey: (apiKey: string) => void;
  clearApiKey: () => void;
};

export const useAppStore = create<AppStore>((set) => ({
  // Dev fallback: use VITE_MEMORYOPS_WORKSPACE_ID when available (local `npm run dev`).
  // In containerised deployments the runtime /config.json (loaded in main.tsx)
  // will override this value before the app renders.
  workspaceId: import.meta.env.VITE_MEMORYOPS_WORKSPACE_ID ?? "",
  apiKey: "",
  setWorkspace: (workspaceId, apiKey) => set({ workspaceId, apiKey }),
  setWorkspaceId: (workspaceId) => set({ workspaceId }),
  setApiKey: (apiKey) => set({ apiKey }),
  clearApiKey: () => set({ apiKey: "" }),
}));

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
    const cfg = (await response.json()) as { workspaceId?: string };
    if (cfg.workspaceId && cfg.workspaceId.trim().length > 0) {
      useAppStore.getState().setWorkspaceId(cfg.workspaceId.trim());
    }
  } catch {
    // Silently ignore — /config.json may not be present in local dev mode.
  }
}
