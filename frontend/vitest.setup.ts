import "@testing-library/jest-dom/vitest";
import { afterEach } from "vitest";

import { clearStoredAppState, useAppStore } from "./src/store/app-store";

class ResizeObserverMock {
  observe() {}
  unobserve() {}
  disconnect() {}
}

if (!globalThis.ResizeObserver) {
  globalThis.ResizeObserver = ResizeObserverMock as typeof ResizeObserver;
}

afterEach(() => {
  useAppStore.setState({ workspaceId: "", apiKey: "" });
  clearStoredAppState();
});
