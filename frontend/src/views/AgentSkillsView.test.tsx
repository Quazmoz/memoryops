import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import type { ReactElement } from "react";
import { vi } from "vitest";

import {
  createAgentSkill,
  getAgentSkill,
  listAgentSkills,
  updateAgentSkill,
  listAgentSkillVersions,
  rollbackAgentSkillVersion,
} from "../api/agentSkills";
import { AgentSkillsView } from "./AgentSkillsView";

vi.mock("../api/agentSkills", () => ({
  createAgentSkill: vi.fn(),
  getAgentSkill: vi.fn(),
  listAgentSkills: vi.fn(),
  updateAgentSkill: vi.fn(),
  listAgentSkillVersions: vi.fn(),
  rollbackAgentSkillVersion: vi.fn(),
}));

const mockCreateAgentSkill = vi.mocked(createAgentSkill);
const mockGetAgentSkill = vi.mocked(getAgentSkill);
const mockListAgentSkills = vi.mocked(listAgentSkills);
const mockUpdateAgentSkill = vi.mocked(updateAgentSkill);
const mockListAgentSkillVersions = vi.mocked(listAgentSkillVersions);
const mockRollbackAgentSkillVersion = vi.mocked(rollbackAgentSkillVersion);

describe("AgentSkillsView", () => {
  beforeEach(() => {
    mockListAgentSkills.mockReset();
    mockGetAgentSkill.mockReset();
    mockCreateAgentSkill.mockReset();
    mockUpdateAgentSkill.mockReset();
    mockListAgentSkillVersions.mockReset();
    mockRollbackAgentSkillVersion.mockReset();
  });

  it("submits a new agent skill from the drawer", async () => {
    mockListAgentSkills.mockResolvedValue([]);
    mockCreateAgentSkill.mockResolvedValue({
      id: "some-uuid",
      assistant: "claude",
      name: "release_notes",
      filename: "release_notes.md",
      title: "Release Notes",
      description: "Summarises release changes",
      instructions: "## Trigger\n- Before deploy",
      content: "# Skill: Release Notes\n\n**Description:** Summarises release changes\n\n## Trigger\n- Before deploy\n",
      version: 1,
    });
    mockGetAgentSkill.mockResolvedValue({
      id: "some-uuid",
      assistant: "claude",
      name: "release_notes",
      filename: "release_notes.md",
      title: "Release Notes",
      description: "Summarises release changes",
      instructions: "## Trigger\n- Before deploy",
      content: "# Skill: Release Notes\n\n**Description:** Summarises release changes\n\n## Trigger\n- Before deploy\n",
      version: 1,
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
        change_note: undefined,
      });
    });
  });

  it("loads an existing skill and submits edits", async () => {
    mockListAgentSkills.mockResolvedValue([
      {
        id: "some-uuid",
        assistant: "claude",
        name: "release_notes",
        filename: "release_notes.md",
        title: "Release Notes",
        description: "Summarises release changes",
        version: 1,
      },
    ]);
    mockGetAgentSkill.mockResolvedValue({
      id: "some-uuid",
      assistant: "claude",
      name: "release_notes",
      filename: "release_notes.md",
      title: "Release Notes",
      description: "Summarises release changes",
      instructions: "## Trigger\n- Before deploy",
      content: "# Skill: Release Notes\n\n**Description:** Summarises release changes\n\n## Trigger\n- Before deploy\n",
      version: 1,
    });
    mockUpdateAgentSkill.mockResolvedValue({
      id: "some-uuid",
      assistant: "claude",
      name: "release_notes",
      filename: "release_notes.md",
      title: "Release Notes",
      description: "Updated release summary",
      instructions: "## Trigger\n- Before and after deploy",
      content: "# Skill: Release Notes\n\n**Description:** Updated release summary\n\n## Trigger\n- Before and after deploy\n",
      version: 2,
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
        change_note: undefined,
      });
    });
  });

  it("displays version history and triggers rollback", async () => {
    const dummySkill = {
      id: "some-uuid",
      assistant: "claude" as const,
      name: "release_notes",
      filename: "release_notes.md",
      title: "Release Notes",
      description: "Summarises release changes",
      version: 2,
    };
    mockListAgentSkills.mockResolvedValue([dummySkill]);
    mockGetAgentSkill.mockResolvedValue({
      ...dummySkill,
      instructions: "## Trigger\n- Before deploy",
      content: "# Skill: Release Notes\n\n**Description:** Summarises release changes\n\n## Trigger\n- Before deploy\n",
    });
    mockListAgentSkillVersions.mockResolvedValue([
      {
        id: "v2-uuid",
        agent_skill_id: "some-uuid",
        workspace_id: "ws-uuid",
        name: "release_notes",
        version: 2,
        assistant: "claude",
        title: "Release Notes",
        description: "Summarises release changes",
        instructions: "## Trigger\n- Before deploy",
        content: "# Skill: Release Notes\n\n**Description:** Summarises release changes\n\n## Trigger\n- Before deploy\n",
        change_note: "second version",
        created_by: "api_key:key1",
        created_at: "2026-06-09T18:00:00Z",
      },
      {
        id: "v1-uuid",
        agent_skill_id: "some-uuid",
        workspace_id: "ws-uuid",
        name: "release_notes",
        version: 1,
        assistant: "claude",
        title: "Release Notes",
        description: "Initial release changes",
        instructions: "## Trigger\n- Before and after deploy",
        content: "# Skill: Release Notes\n\n**Description:** Initial release changes\n\n## Trigger\n- Before and after deploy\n",
        change_note: "initial commit",
        created_by: "api_key:key1",
        created_at: "2026-06-09T17:00:00Z",
      },
    ]);
    mockRollbackAgentSkillVersion.mockResolvedValue({
      ...dummySkill,
      version: 3,
      instructions: "## Trigger\n- Before and after deploy",
      content: "# Skill: Release Notes\n\n**Description:** Summarises release changes\n\n## Trigger\n- Before and after deploy\n",
    });

    renderWithQueryClient(<AgentSkillsView />);

    fireEvent.click(await screen.findByRole("button", { name: /release notes/i }));
    
    // Switch to Version History tab
    fireEvent.click(await screen.findByRole("button", { name: /version history/i }));

    expect(await screen.findByText("second version")).toBeInTheDocument();
    expect(await screen.findByText("initial commit")).toBeInTheDocument();

    // Click rollback button for v1
    fireEvent.click(screen.getByRole("button", { name: /roll back/i }));
    fireEvent.click(screen.getByRole("button", { name: /^confirm$/i }));

    await waitFor(() => {
      expect(mockRollbackAgentSkillVersion).toHaveBeenCalledWith("claude", "release_notes", 1, undefined);
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
