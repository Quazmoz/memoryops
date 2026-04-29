import { render, screen } from "@testing-library/react";
import { vi } from "vitest";

import { useAppStore } from "../store/app-store";
import { CodeBlock } from "./CodeBlock";

vi.mock("../store/app-store", () => ({
  useAppStore: vi.fn(),
}));

const mockUseAppStore = vi.mocked(useAppStore) as unknown as ReturnType<typeof vi.fn>;

function setupStore(workspaceId: string, apiKey: string) {
  mockUseAppStore.mockImplementation(
    (selector: (s: { workspaceId: string; apiKey: string }) => string) =>
      selector({ workspaceId, apiKey }),
  );
}

describe("CodeBlock", () => {
  it("renders the copy button", () => {
    setupStore("", "");
    render(<CodeBlock code="hello world" />);
    expect(screen.getByRole("button", { name: /copy code/i })).toBeTruthy();
  });

  it("substitutes {{WORKSPACE_ID}} with store value", () => {
    setupStore("ws-abc-123", "");
    render(<CodeBlock code="workspace: {{WORKSPACE_ID}}" />);
    expect(screen.getByText(/workspace: ws-abc-123/i)).toBeTruthy();
  });

  it("substitutes {{API_KEY}} with store value", () => {
    setupStore("", "sk-live-xyz");
    render(<CodeBlock code="key: {{API_KEY}}" />);
    expect(screen.getByText(/key: sk-live-xyz/i)).toBeTruthy();
  });

  it("shows placeholder when workspace ID is empty", () => {
    setupStore("", "");
    render(<CodeBlock code="id={{WORKSPACE_ID}}" />);
    expect(screen.getByText(/id=<YOUR_WORKSPACE_ID>/i)).toBeTruthy();
  });

  it("shows placeholder when API key is empty", () => {
    setupStore("", "");
    render(<CodeBlock code="key={{API_KEY}}" />);
    expect(screen.getByText(/key=<YOUR_API_KEY>/i)).toBeTruthy();
  });

  it("substitutes both placeholders in a single code block", () => {
    setupStore("my-ws", "my-key");
    render(<CodeBlock code="-H 'x-api-key: {{API_KEY}}' /v1/workspaces/{{WORKSPACE_ID}}" />);
    const pre = screen.getByTestId("code-block").querySelector("code");
    expect(pre?.textContent).toContain("my-ws");
    expect(pre?.textContent).toContain("my-key");
  });
});
