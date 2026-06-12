import { fireEvent, render, screen } from "@testing-library/react";
import type { ReactElement } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";

import type { MemoryUnit } from "../api/types";
import { useDeleteMemory, usePromoteMemory, useRestoreMemory } from "../hooks/use-memory";
import { MemoryLifecycleActions } from "./MemoryLifecycleActions";
import { TooltipProvider } from "./ui/tooltip";

vi.mock("../hooks/use-memory", () => ({
  useDeleteMemory: vi.fn(),
  usePromoteMemory: vi.fn(),
  useRestoreMemory: vi.fn(),
}));

const mockUseDeleteMemory = vi.mocked(useDeleteMemory);
const mockUsePromoteMemory = vi.mocked(usePromoteMemory);
const mockUseRestoreMemory = vi.mocked(useRestoreMemory);

const WORKSPACE_ID = "018f0000-0000-7000-8000-000000000001";

function mutationState(overrides: Record<string, unknown> = {}) {
  return {
    mutate: vi.fn(),
    isPending: false,
    isError: false,
    error: null,
    ...overrides,
  } as unknown as ReturnType<typeof useDeleteMemory>;
}

function memoryFixture(overrides: Partial<MemoryUnit> = {}): MemoryUnit {
  return {
    id: "mem-1",
    workspace_id: WORKSPACE_ID,
    scope: null,
    memory_type: "episodic",
    scope_visibility: "private",
    content: "memory content",
    importance_score: 0.8,
    decay_score: 1,
    relevance_score: 0.5,
    pinned: false,
    tags: [],
    source_events: [],
    source_episode_ids: [],
    corroboration_count: 0,
    created_at: "2026-01-01T00:00:00Z",
    updated_at: "2026-01-01T00:00:00Z",
    ...overrides,
  };
}

function renderWithTooltip(ui: ReactElement) {
  return render(<TooltipProvider>{ui}</TooltipProvider>);
}

describe("MemoryLifecycleActions", () => {
  beforeEach(() => {
    mockUseDeleteMemory.mockReturnValue(mutationState());
    mockUsePromoteMemory.mockReturnValue(mutationState());
    mockUseRestoreMemory.mockReturnValue(mutationState());
  });

  it("shows the promote action for episodic memories", () => {
    renderWithTooltip(<MemoryLifecycleActions workspaceId={WORKSPACE_ID} memory={memoryFixture()} />);

    expect(screen.getByTestId("memory-promote-button")).toBeTruthy();
  });

  it("hides the promote action for semantic memories", () => {
    renderWithTooltip(
      <MemoryLifecycleActions workspaceId={WORKSPACE_ID} memory={memoryFixture({ memory_type: "semantic" })} />,
    );

    expect(screen.queryByTestId("memory-promote-button")).toBeNull();
  });

  it("calls the promote mutation with the memory id", () => {
    const promote = mutationState();
    mockUsePromoteMemory.mockReturnValue(promote);

    renderWithTooltip(<MemoryLifecycleActions workspaceId={WORKSPACE_ID} memory={memoryFixture()} />);
    fireEvent.click(screen.getByTestId("memory-promote-button"));

    expect(promote.mutate).toHaveBeenCalledWith({ id: "mem-1" });
  });

  it("requires confirmation before deleting", () => {
    const remove = mutationState();
    mockUseDeleteMemory.mockReturnValue(remove);

    renderWithTooltip(<MemoryLifecycleActions workspaceId={WORKSPACE_ID} memory={memoryFixture()} />);

    fireEvent.click(screen.getByTestId("memory-delete-button"));
    expect(remove.mutate).not.toHaveBeenCalled();
    expect(screen.getByTestId("memory-delete-confirm")).toBeTruthy();

    fireEvent.click(screen.getByTestId("memory-delete-confirm-button"));
    expect(remove.mutate).toHaveBeenCalledTimes(1);
    expect(remove.mutate).toHaveBeenCalledWith({ id: "mem-1" }, expect.anything());
  });

  it("offers restore after a successful delete", () => {
    const restore = mutationState();
    const remove = mutationState({
      mutate: vi.fn((_variables: { id: string }, options?: { onSuccess?: () => void }) => {
        options?.onSuccess?.();
      }),
    });
    mockUseDeleteMemory.mockReturnValue(remove);
    mockUseRestoreMemory.mockReturnValue(restore);

    renderWithTooltip(<MemoryLifecycleActions workspaceId={WORKSPACE_ID} memory={memoryFixture()} />);

    fireEvent.click(screen.getByTestId("memory-delete-button"));
    fireEvent.click(screen.getByTestId("memory-delete-confirm-button"));

    expect(screen.getByTestId("memory-deleted-banner")).toBeTruthy();

    fireEvent.click(screen.getByTestId("memory-restore-button"));
    expect(restore.mutate).toHaveBeenCalledWith({ id: "mem-1" }, expect.anything());
  });
});
