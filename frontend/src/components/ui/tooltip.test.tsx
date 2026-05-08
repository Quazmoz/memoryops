import { fireEvent, render, screen, waitFor } from "@testing-library/react";

import { HelpTooltip, InfoLabel, TooltipProvider } from "./tooltip";

describe("HelpTooltip", () => {
  it("shows tooltip content on focus", async () => {
    render(
      <TooltipProvider delayDuration={0}>
        <HelpTooltip label="Memory health">Shows decay, deletion, and age signals for the workspace memory pool.</HelpTooltip>
      </TooltipProvider>,
    );

    fireEvent.focus(screen.getByLabelText("Help: Memory health"));

    await waitFor(() => {
      expect(screen.getAllByText("Shows decay, deletion, and age signals for the workspace memory pool.").length).toBeGreaterThan(0);
    });
  });

  it("renders an inline label and accessible help trigger", () => {
    render(
      <TooltipProvider delayDuration={0}>
        <InfoLabel
          label="Promotion threshold"
          tooltip="Minimum confidence required before related episodic memories are promoted into semantic memory."
        />
      </TooltipProvider>,
    );

    expect(screen.getByText("Promotion threshold")).toBeInTheDocument();
    expect(screen.getByLabelText("Help: Promotion threshold")).toBeInTheDocument();
  });
});
