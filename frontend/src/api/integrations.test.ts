import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { createIntegration, deleteIntegration, INTEGRATION_SOURCES } from "./integrations";
import { useAppStore } from "../store/app-store";

const WORKSPACE_ID = "018f0000-0000-7000-8000-000000000001";

const fetchMock = vi.fn();

function jsonResponse(body: unknown, status = 200): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { "content-type": "application/json" },
  });
}

describe("integrations api", () => {
  beforeEach(() => {
    vi.stubGlobal("fetch", fetchMock);
    fetchMock.mockReset();
    useAppStore.setState({ workspaceId: WORKSPACE_ID, apiKey: "mops_test_key" });
  });

  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("exposes the backend Source enum values", () => {
    expect(INTEGRATION_SOURCES).toEqual(["github", "slack", "jira", "linear", "observation"]);
  });

  it("creates an integration with a trimmed webhook secret", async () => {
    const integration = {
      source: "github",
      last_event_at: null,
      events_24h: 0,
      errors_24h: 0,
      status: "active",
    };
    fetchMock.mockResolvedValueOnce(jsonResponse(integration));

    const result = await createIntegration(WORKSPACE_ID, {
      source: "github",
      webhook_secret: "  super-secret  ",
    });

    expect(result).toEqual(integration);
    expect(fetchMock).toHaveBeenCalledTimes(1);
    const [url, init] = fetchMock.mock.calls[0] as [string, RequestInit];
    expect(url).toBe(`/api/v1/workspaces/${WORKSPACE_ID}/integrations`);
    expect(init.method).toBe("POST");
    expect(JSON.parse(init.body as string)).toEqual({
      source: "github",
      webhook_secret: "super-secret",
    });
  });

  it("rejects creation without a webhook secret and never calls the API", async () => {
    await expect(
      createIntegration(WORKSPACE_ID, { source: "slack", webhook_secret: "   " }),
    ).rejects.toThrow("Webhook secret is required");
    expect(fetchMock).not.toHaveBeenCalled();
  });

  it("deletes an integration with an encoded source path", async () => {
    fetchMock.mockResolvedValueOnce(new Response(null, { status: 204 }));

    await deleteIntegration(WORKSPACE_ID, "github");

    const [url, init] = fetchMock.mock.calls[0] as [string, RequestInit];
    expect(url).toBe(`/api/v1/workspaces/${WORKSPACE_ID}/integrations/github`);
    expect(init.method).toBe("DELETE");
  });

  it("surfaces backend validation errors", async () => {
    fetchMock.mockResolvedValueOnce(jsonResponse({ detail: "webhook_secret is required" }, 400));

    await expect(
      createIntegration(WORKSPACE_ID, { source: "jira", webhook_secret: "secret" }),
    ).rejects.toThrow("webhook_secret is required");
  });
});
