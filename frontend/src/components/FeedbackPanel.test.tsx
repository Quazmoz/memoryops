import { render, screen } from "@testing-library/react";
import type { ReactElement } from "react";
import { vi } from "vitest";

import { useMemoryFeedback, useSubmitFeedback } from "../hooks/use-memory";
import { TooltipProvider } from "./ui/tooltip";
import { FeedbackPanel } from "./FeedbackPanel";

vi.mock("../hooks/use-memory", () => ({
  useMemoryFeedback: vi.fn(),
  useSubmitFeedback: vi.fn(),
}));

const mockUseMemoryFeedback = vi.mocked(useMemoryFeedback);
const mockUseSubmitFeedback = vi.mocked(useSubmitFeedback);

describe("FeedbackPanel", () => {
  beforeEach(() => {
    mockUseMemoryFeedback.mockReturnValue(feedbackQueryState());
    mockUseSubmitFeedback.mockReturnValue(submitFeedbackState());
  });

  it("renders three rating buttons", () => {
    renderWithTooltip(<FeedbackPanel workspaceId="workspace-1" memoryId="memory-1" />);

    expect(screen.getByRole("button", { name: /positive feedback/i })).toBeTruthy();
    expect(screen.getByRole("button", { name: /neutral feedback/i })).toBeTruthy();
    expect(screen.getByRole("button", { name: /negative feedback/i })).toBeTruthy();
  });

  it("disables submit when mutation is pending", () => {
    mockUseSubmitFeedback.mockReturnValue(submitFeedbackState({ isPending: true }));

    renderWithTooltip(<FeedbackPanel workspaceId="workspace-1" memoryId="memory-1" />);

    expect((screen.getByTestId("feedback-submit") as HTMLButtonElement).disabled).toBe(true);
  });
});

function feedbackQueryState(overrides: Record<string, unknown> = {}) {
  return {
    data: {
      items: [],
      total: 0,
      memory_id: "memory-1",
      avg_rating: 0,
      relevance_score: 0.5,
    },
    error: null,
    isError: false,
    isLoading: false,
    ...overrides,
  } as unknown as ReturnType<typeof useMemoryFeedback>;
}

function submitFeedbackState(overrides: Record<string, unknown> = {}) {
  return {
    error: null,
    isError: false,
    isPending: false,
    isSuccess: false,
    mutate: vi.fn(),
    ...overrides,
  } as unknown as ReturnType<typeof useSubmitFeedback>;
}

function renderWithTooltip(ui: ReactElement) {
  return render(<TooltipProvider delayDuration={0}>{ui}</TooltipProvider>);
}
