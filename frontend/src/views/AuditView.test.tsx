import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import type { ReactElement } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";

import {
  downloadAuditExport,
  listAuditActions,
  listAuditEvents,
  verifyAuditChain,
} from "../api/audit";
import type { AuditEvent } from "../api/types";
import { TooltipProvider } from "../components/ui/tooltip";
import { useAppStore } from "../store/app-store";
import { AuditView } from "./AuditView";

vi.mock("../api/audit", () => ({
  listAuditEvents: vi.fn(),
  listAuditActions: vi.fn(),
  verifyAuditChain: vi.fn(),
  downloadAuditExport: vi.fn(),
}));

const mockListAuditEvents = vi.mocked(listAuditEvents);
const mockListAuditActions = vi.mocked(listAuditActions);
const mockVerifyAuditChain = vi.mocked(verifyAuditChain);
const mockDownloadAuditExport = vi.mocked(downloadAuditExport);

const WORKSPACE_ID = "018f0000-0000-7000-8000-000000000001";

function auditEvent(overrides: Partial<AuditEvent> = {}): AuditEvent {
  return {
    id: "018f-audit-1",
    workspace_id: WORKSPACE_ID,
    actor: "api_key:alpha",
    action: "integration_updated",
    target_id: "018f-target-1",
    target_type: "integration",
    occurred_at: "2026-06-18T10:00:00Z",
    severity: "notice",
    success: true,
    // The backend redacts payloads before they reach the client; the view must
    // surface exactly what it receives and never reconstruct a secret.
    metadata: { webhook_secret: "[REDACTED]", endpoint: "https://example.com" },
    source_ip: "203.0.113.9",
    ...overrides,
  };
}

describe("AuditView", () => {
  beforeEach(() => {
    mockListAuditEvents.mockReset();
    mockListAuditActions.mockReset();
    mockVerifyAuditChain.mockReset();
    mockDownloadAuditExport.mockReset();
    useAppStore.setState({ workspaceId: WORKSPACE_ID, apiKey: "test-api-key" });

    mockListAuditActions.mockResolvedValue({
      actions: [
        { name: "integration_updated", category: "integration", default_severity: "notice", required: true },
        { name: "tool_invoked", category: "tool", default_severity: "info", required: false },
      ],
      severities: ["info", "notice", "warning", "critical"],
      categories: ["integration", "tool"],
    });
    mockListAuditEvents.mockResolvedValue({
      items: [auditEvent()],
      limit: 50,
      offset: 0,
      next_cursor: null,
    });
  });

  it("renders audit rows and never exposes secrets when a row is expanded", async () => {
    renderView();

    // Row content from the list query (scoped to the table — the action name
    // also appears as an option in the Action filter dropdown).
    const table = await screen.findByRole("table");
    expect(within(table).getByText("integration_updated")).toBeInTheDocument();
    expect(within(table).getByText("203.0.113.9")).toBeInTheDocument();

    // Expand the row to reveal the detail panel.
    fireEvent.click(screen.getByLabelText("Expand"));

    // Redacted payloads are shown verbatim, with the redaction disclaimer.
    expect(screen.getByText(/payloads are redacted/i)).toBeInTheDocument();
    expect(document.body.textContent).toContain("[REDACTED]");
    expect(document.body.textContent).toContain("https://example.com");
  });

  it("maps the search filter to the backend query on Apply", async () => {
    renderView();
    await screen.findByText("203.0.113.9");

    fireEvent.change(screen.getByPlaceholderText(/actor, target, request id/i), {
      target: { value: "deploy" },
    });
    fireEvent.click(screen.getByRole("button", { name: /^apply$/i }));

    await waitFor(() => {
      expect(mockListAuditEvents).toHaveBeenLastCalledWith(
        WORKSPACE_ID,
        expect.objectContaining({ q: "deploy", limit: 50 }),
      );
    });
  });

  it("triggers JSONL and CSV exports with the applied filters", async () => {
    mockDownloadAuditExport.mockResolvedValue(undefined);
    renderView();
    await screen.findByText("203.0.113.9");

    fireEvent.click(screen.getByRole("button", { name: /jsonl/i }));
    fireEvent.click(screen.getByRole("button", { name: /csv/i }));

    await waitFor(() => {
      expect(mockDownloadAuditExport).toHaveBeenCalledWith(WORKSPACE_ID, "jsonl", expect.any(Object));
      expect(mockDownloadAuditExport).toHaveBeenCalledWith(WORKSPACE_ID, "csv", expect.any(Object));
    });
  });

  it("shows the verification result after Verify", async () => {
    mockVerifyAuditChain.mockResolvedValue({
      enabled: true,
      verified: true,
      checked: 7,
      first_broken_seq: null,
      message: "verified 7 hashed audit rows",
    });
    renderView();
    await screen.findByText("203.0.113.9");

    fireEvent.click(screen.getByRole("button", { name: /verify/i }));

    expect(await screen.findByText(/verified 7 hashed audit rows/i)).toBeInTheDocument();
    expect(mockVerifyAuditChain).toHaveBeenCalledWith(WORKSPACE_ID);
  });
});

function renderView(): ReactElement {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  return render(
    <QueryClientProvider client={queryClient}>
      <TooltipProvider delayDuration={0}>
        <AuditView />
      </TooltipProvider>
    </QueryClientProvider>,
  ) as unknown as ReactElement;
}
