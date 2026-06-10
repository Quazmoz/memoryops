import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { MemoryRouter } from "react-router-dom";
import type { ReactElement } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { deleteWorkspace } from "../api/workspaces";
import { useAppStore } from "../store/app-store";
import { WorkspaceDangerZone } from "./WorkspaceDangerZone";

vi.mock("../api/workspaces", () => ({
  deleteWorkspace: vi.fn(),
}));

const mockDeleteWorkspace = vi.mocked(deleteWorkspace);

const WORKSPACE_ID = "018f0000-0000-7000-8000-000000000001";

function renderDangerZone(ui: ReactElement) {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });

  return render(
    <QueryClientProvider client={queryClient}>
      <MemoryRouter>{ui}</MemoryRouter>
    </QueryClientProvider>,
  );
}

describe("WorkspaceDangerZone", () => {
  beforeEach(() => {
    mockDeleteWorkspace.mockReset();
    useAppStore.setState({ workspaceId: WORKSPACE_ID, apiKey: "mops_key" });
  });

  it("requires typing the exact workspace name before enabling deletion", () => {
    renderDangerZone(<WorkspaceDangerZone workspaceId={WORKSPACE_ID} workspaceName="Production" />);

    fireEvent.click(screen.getByTestId("delete-workspace-button"));

    const confirmButton = screen.getByTestId("delete-workspace-confirm-button") as HTMLButtonElement;
    expect(confirmButton.disabled).toBe(true);

    fireEvent.change(screen.getByTestId("delete-workspace-confirm-input"), {
      target: { value: "production" },
    });
    expect(confirmButton.disabled).toBe(true);

    fireEvent.change(screen.getByTestId("delete-workspace-confirm-input"), {
      target: { value: "Production" },
    });
    expect(confirmButton.disabled).toBe(false);
    expect(mockDeleteWorkspace).not.toHaveBeenCalled();
  });

  it("falls back to the workspace ID when no name is available", () => {
    renderDangerZone(<WorkspaceDangerZone workspaceId={WORKSPACE_ID} />);

    fireEvent.click(screen.getByTestId("delete-workspace-button"));

    const confirmButton = screen.getByTestId("delete-workspace-confirm-button") as HTMLButtonElement;
    expect(confirmButton.disabled).toBe(true);

    fireEvent.change(screen.getByTestId("delete-workspace-confirm-input"), {
      target: { value: WORKSPACE_ID },
    });
    expect(confirmButton.disabled).toBe(false);
  });

  it("deletes the workspace and clears stored credentials on success", async () => {
    mockDeleteWorkspace.mockResolvedValueOnce({ deleted: true });

    renderDangerZone(<WorkspaceDangerZone workspaceId={WORKSPACE_ID} workspaceName="Production" />);

    fireEvent.click(screen.getByTestId("delete-workspace-button"));
    fireEvent.change(screen.getByTestId("delete-workspace-confirm-input"), {
      target: { value: "Production" },
    });
    fireEvent.click(screen.getByTestId("delete-workspace-confirm-button"));

    await waitFor(() => {
      expect(mockDeleteWorkspace).toHaveBeenCalledWith(WORKSPACE_ID);
    });
    await waitFor(() => {
      expect(useAppStore.getState().workspaceId).toBe("");
      expect(useAppStore.getState().apiKey).toBe("");
    });
  });

  it("keeps credentials and shows an error when deletion fails", async () => {
    mockDeleteWorkspace.mockRejectedValueOnce(new Error("backend exploded"));

    renderDangerZone(<WorkspaceDangerZone workspaceId={WORKSPACE_ID} workspaceName="Production" />);

    fireEvent.click(screen.getByTestId("delete-workspace-button"));
    fireEvent.change(screen.getByTestId("delete-workspace-confirm-input"), {
      target: { value: "Production" },
    });
    fireEvent.click(screen.getByTestId("delete-workspace-confirm-button"));

    await waitFor(() => {
      expect(screen.getByText("backend exploded")).toBeTruthy();
    });
    expect(useAppStore.getState().workspaceId).toBe(WORKSPACE_ID);
    expect(useAppStore.getState().apiKey).toBe("mops_key");
  });
});
