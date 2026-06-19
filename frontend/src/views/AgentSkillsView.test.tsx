import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import type { ReactElement } from "react";
import { vi } from "vitest";

import {
  createAgentResource,
  deleteAgentResource,
  getAgentResource,
  listAgentResources,
  listAgentResourceVersions,
  rollbackAgentResource,
  updateAgentResource,
} from "../api/agentResources";
import { AgentSkillsView } from "./AgentSkillsView";

vi.mock("../api/agentResources", () => ({
  createAgentResource: vi.fn(),
  deleteAgentResource: vi.fn(),
  getAgentResource: vi.fn(),
  listAgentResources: vi.fn(),
  listAgentResourceVersions: vi.fn(),
  rollbackAgentResource: vi.fn(),
  updateAgentResource: vi.fn(),
}));

const mockCreateAgentResource = vi.mocked(createAgentResource);
const mockDeleteAgentResource = vi.mocked(deleteAgentResource);
const mockGetAgentResource = vi.mocked(getAgentResource);
const mockListAgentResources = vi.mocked(listAgentResources);
const mockListAgentResourceVersions = vi.mocked(listAgentResourceVersions);
const mockRollbackAgentResource = vi.mocked(rollbackAgentResource);
const mockUpdateAgentResource = vi.mocked(updateAgentResource);

describe("AgentSkillsView", () => {
  beforeEach(() => {
    mockCreateAgentResource.mockReset();
    mockDeleteAgentResource.mockReset();
    mockGetAgentResource.mockReset();
    mockListAgentResources.mockReset();
    mockListAgentResourceVersions.mockReset();
    mockRollbackAgentResource.mockReset();
    mockUpdateAgentResource.mockReset();
  });

  it("submits a new skill resource from the drawer", async () => {
    mockListAgentResources.mockResolvedValue([]);
    mockListAgentResourceVersions.mockResolvedValue([]);
    mockCreateAgentResource.mockResolvedValue(releaseNotesResource());
    mockGetAgentResource.mockResolvedValue(releaseNotesResource());

    renderWithQueryClient(<AgentSkillsView />);

    fireEvent.click(await screen.findByRole("button", { name: /add resource/i }));
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
    fireEvent.change(within(dialog).getByRole("textbox", { name: /body/i }), {
      target: { value: "## Trigger\n- Before deploy" },
    });

    fireEvent.click(within(dialog).getByRole("button", { name: /create resource/i }));

    await waitFor(() => {
      expect(mockCreateAgentResource).toHaveBeenCalledWith(expect.objectContaining({
        assistant: "claude",
        body: "## Trigger\n- Before deploy",
        description: "Summarises release changes",
        kind: "skill",
        name: "release_notes",
        title: "Release Notes",
      }));
    });
  });

  it("submits metadata for a prompt resource from the drawer", async () => {
    mockListAgentResources.mockResolvedValue([]);
    mockListAgentResourceVersions.mockResolvedValue([]);
    mockCreateAgentResource.mockResolvedValue({
      ...releaseNotesResource(),
      kind: "prompt",
      assistant: "generic",
      name: "release_brief",
      title: "Release Brief",
      description: "Drafts release notes",
      metadata: { owner: "release" },
      content: "# Prompt: Release Brief\n\n**Description:** Drafts release notes\n\n## Prompt\nSummarize changes.\n",
    });
    mockGetAgentResource.mockResolvedValue(releaseNotesResource());

    renderWithQueryClient(<AgentSkillsView />);

    fireEvent.click(await screen.findByRole("button", { name: /add resource/i }));
    const dialog = screen.getByRole("dialog");
    fireEvent.change(within(dialog).getByLabelText(/type/i), {
      target: { value: "prompt" },
    });
    fireEvent.change(within(dialog).getByPlaceholderText("release_notes"), {
      target: { value: "release_brief" },
    });
    fireEvent.change(within(dialog).getByPlaceholderText("Release Brief Prompt"), {
      target: { value: "Release Brief" },
    });
    fireEvent.change(within(dialog).getByPlaceholderText("Drafts concise release notes from merged changes."), {
      target: { value: "Drafts release notes" },
    });
    fireEvent.change(within(dialog).getByRole("textbox", { name: /body/i }), {
      target: { value: "## Prompt\nSummarize changes." },
    });
    fireEvent.change(within(dialog).getByRole("textbox", { name: /metadata/i }), {
      target: { value: '{ "owner": "release", "default": false }' },
    });

    fireEvent.click(within(dialog).getByRole("button", { name: /create resource/i }));

    await waitFor(() => {
      expect(mockCreateAgentResource).toHaveBeenCalledWith(expect.objectContaining({
        assistant: "generic",
        kind: "prompt",
        metadata: { owner: "release", default: false },
        name: "release_brief",
      }));
    });
  });

  it("loads an existing resource and submits edits", async () => {
    mockListAgentResources.mockResolvedValue([releaseNotesSummary()]);
    mockGetAgentResource.mockResolvedValue(releaseNotesResource());
    mockListAgentResourceVersions.mockResolvedValue([
      {
        ...releaseNotesResource(),
        id: "version-1",
        resource_id: "resource-1",
        version: 1,
        change_note: "Initial version",
        created_by: "api_key:test",
        created_at: "2026-06-18T10:00:00Z",
      },
    ]);
    mockUpdateAgentResource.mockResolvedValue({
      ...releaseNotesResource(),
      description: "Updated release summary",
      body: "## Trigger\n- Before and after deploy",
      content: "# Skill: Release Notes\n\n**Description:** Updated release summary\n\n## Trigger\n- Before and after deploy\n",
      version: 2,
    });

    renderWithQueryClient(<AgentSkillsView />);

    fireEvent.click(await screen.findByRole("button", { name: /release notes/i }));
    await waitFor(() => {
      expect(screen.getByRole("button", { name: /^edit$/i })).not.toBeDisabled();
    });

    fireEvent.click(screen.getByRole("button", { name: /^edit$/i }));
    const dialog = screen.getByRole("dialog");
    fireEvent.change(within(dialog).getByDisplayValue("Summarises release changes"), {
      target: { value: "Updated release summary" },
    });
    fireEvent.change(within(dialog).getByRole("textbox", { name: /body/i }), {
      target: { value: "## Trigger\n- Before and after deploy" },
    });

    fireEvent.click(within(dialog).getByRole("button", { name: /^save resource$/i }));

    await waitFor(() => {
      expect(mockUpdateAgentResource).toHaveBeenCalledWith("skill", "claude", "release_notes", expect.objectContaining({
        body: "## Trigger\n- Before and after deploy",
        description: "Updated release summary",
        title: "Release Notes",
      }));
    });
  });
});

function releaseNotesSummary() {
  return {
    id: "resource-1",
    workspace_id: "workspace-1",
    kind: "skill" as const,
    assistant: "claude" as const,
    name: "release_notes",
    filename: "release_notes.md",
    title: "Release Notes",
    description: "Summarises release changes",
    metadata: {},
    version: 1,
    created_at: "2026-06-18T10:00:00Z",
    updated_at: "2026-06-18T10:00:00Z",
  };
}

function releaseNotesResource() {
  return {
    ...releaseNotesSummary(),
    body: "## Trigger\n- Before deploy",
    content: "# Skill: Release Notes\n\n**Description:** Summarises release changes\n\n## Trigger\n- Before deploy\n",
  };
}

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
