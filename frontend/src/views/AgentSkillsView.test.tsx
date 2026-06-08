import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import type { ReactElement } from "react";
import { vi } from "vitest";

import {
  createAgentSkill,
  getAgentSkill,
  listAgentSkills,
  updateAgentSkill,
} from "../api/agentSkills";
import { AgentSkillsView } from "./AgentSkillsView";

vi.mock("../api/agentSkills", () => ({
  createAgentSkill: vi.fn(),
  getAgentSkill: vi.fn(),
  listAgentSkills: vi.fn(),
  updateAgentSkill: vi.fn(),
}));

const mockCreateAgentSkill = vi.mocked(createAgentSkill);
const mockGetAgentSkill = vi.mocked(getAgentSkill);
const mockListAgentSkills = vi.mocked(listAgentSkills);
const mockUpdateAgentSkill = vi.mocked(updateAgentSkill);

describe("AgentSkillsView", () => {
  beforeEach(() => {
    mockListAgentSkills.mockReset();
    mockGetAgentSkill.mockReset();
    mockCreateAgentSkill.mockReset();
    mockUpdateAgentSkill.mockReset();
  });

  it("submits a new agent skill from the drawer", async () => {
    mockListAgentSkills.mockResolvedValue([]);
    mockCreateAgentSkill.mockResolvedValue({
      assistant: "claude",
      name: "release_notes",
      filename: "release_notes.md",
      title: "Release Notes",
      description: "Summarises release changes",
      instructions: "## Trigger\n- Before deploy",
      content: "# Skill: Release Notes\n\n**Description:** Summarises release changes\n\n## Trigger\n- Before deploy\n",
    });
    mockGetAgentSkill.mockResolvedValue({
      assistant: "claude",
      name: "release_notes",
      filename: "release_notes.md",
      title: "Release Notes",
      description: "Summarises release changes",
      instructions: "## Trigger\n- Before deploy",
      content: "# Skill: Release Notes\n\n**Description:** Summarises release changes\n\n## Trigger\n- Before deploy\n",
    });

    renderWithQueryClient(<AgentSkillsView />);

    fireEvent.click(await screen.findByRole("button", { name: /add skill/i }));
    const dialog = screen.getByRole("dialog");
    fireEvent.change(within(dialog).getByPlaceholderText("release_notes"), {
      target: { value: "release_notes" },
    });
    fireEvent.change(within(dialog).getByPlaceholderText("Release Notes Assistant"), {
      target: { value: "Release Notes" },
    });
    fireEvent.change(
      within(dialog).getByPlaceholderText("Summarises release notes and deployment changes."),
      {
        target: { value: "Summarises release changes" },
      },
    );
    fireEvent.change(within(dialog).getByRole("textbox", { name: /instructions/i }), {
      target: { value: "## Trigger\n- Before deploy" },
    });

    fireEvent.click(within(dialog).getByRole("button", { name: /create skill/i }));

    await waitFor(() => {
      expect(mockCreateAgentSkill).toHaveBeenCalledWith({
        assistant: "claude",
        name: "release_notes",
        title: "Release Notes",
        description: "Summarises release changes",
        instructions: "## Trigger\n- Before deploy",
      });
    });
  });

  it("loads an existing skill and submits edits", async () => {
    mockListAgentSkills.mockResolvedValue([
      {
        assistant: "claude",
        name: "release_notes",
        filename: "release_notes.md",
        title: "Release Notes",
        description: "Summarises release changes",
      },
    ]);
    mockGetAgentSkill.mockResolvedValue({
      assistant: "claude",
      name: "release_notes",
      filename: "release_notes.md",
      title: "Release Notes",
      description: "Summarises release changes",
      instructions: "## Trigger\n- Before deploy",
      content: "# Skill: Release Notes\n\n**Description:** Summarises release changes\n\n## Trigger\n- Before deploy\n",
    });
    mockUpdateAgentSkill.mockResolvedValue({
      assistant: "claude",
      name: "release_notes",
      filename: "release_notes.md",
      title: "Release Notes",
      description: "Updated release summary",
      instructions: "## Trigger\n- Before and after deploy",
      content: "# Skill: Release Notes\n\n**Description:** Updated release summary\n\n## Trigger\n- Before and after deploy\n",
    });

    renderWithQueryClient(<AgentSkillsView />);

    fireEvent.click(await screen.findByRole("button", { name: /release notes/i }));
    await waitFor(() => {
      expect(screen.getByRole("button", { name: /edit skill/i })).not.toBeDisabled();
    });

    fireEvent.click(screen.getByRole("button", { name: /edit skill/i }));
    const dialog = screen.getByRole("dialog");
    fireEvent.change(within(dialog).getByDisplayValue("Summarises release changes"), {
      target: { value: "Updated release summary" },
    });
    fireEvent.change(within(dialog).getByRole("textbox", { name: /instructions/i }), {
      target: { value: "## Trigger\n- Before and after deploy" },
    });

    fireEvent.click(within(dialog).getByRole("button", { name: /^save skill$/i }));

    await waitFor(() => {
      expect(mockUpdateAgentSkill).toHaveBeenCalledWith("claude", "release_notes", {
        title: "Release Notes",
        description: "Updated release summary",
        instructions: "## Trigger\n- Before and after deploy",
      });
    });
  });
});

function renderWithQueryClient(ui: ReactElement) {
  const queryClient = new QueryClient({
    defaultOptions: {
      queries: {
        retry: false,
      },
      mutations: {
        retry: false,
      },
    },
  });

  return render(<QueryClientProvider client={queryClient}>{ui}</QueryClientProvider>);
}
