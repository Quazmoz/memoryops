import { create } from "zustand";

const DEFAULT_WORKSPACE_ID = "00000000-0000-7000-8000-000000000001";

type AppStore = {
  workspaceId: string;
  apiKey: string;
  setWorkspaceId: (workspaceId: string) => void;
  setApiKey: (apiKey: string) => void;
  clearApiKey: () => void;
};

export const useAppStore = create<AppStore>((set) => ({
  workspaceId: import.meta.env.VITE_MEMORYOPS_WORKSPACE_ID ?? DEFAULT_WORKSPACE_ID,
  apiKey: "",
  setWorkspaceId: (workspaceId) => set({ workspaceId }),
  setApiKey: (apiKey) => set({ apiKey }),
  clearApiKey: () => set({ apiKey: "" }),
}));
