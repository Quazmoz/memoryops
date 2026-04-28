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
  workspaceId: import.meta.env.VITE_MEMORYOPS_WORKSPACE_ID ?? "",
  apiKey: "",
  setWorkspace: (workspaceId, apiKey) => set({ workspaceId, apiKey }),
  setWorkspaceId: (workspaceId) => set({ workspaceId }),
  setApiKey: (apiKey) => set({ apiKey }),
  clearApiKey: () => set({ apiKey: "" }),
}));
