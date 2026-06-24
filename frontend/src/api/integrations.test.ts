import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { createIntegration, deleteIntegration, INTEGRATION_SOURCES, startConnectorSync } from "./integrations";
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
    useAppStore.setState({ workspaceId: WORKSPACE_ID, apiKey: "test-api-key" });
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
      has_webhook_secret: true,
      has_api_credential: false,
      api_sync_enabled: false,
      sync_config: {},
      last_sync_at: null,
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

  it("creates an API sync integration with a trimmed platform token", async () => {
    const integration = {
      source: "github",
      last_event_at: null,
      events_24h: 0,
      errors_24h: 0,
      status: "active",
      has_webhook_secret: false,
      has_api_credential: true,
      api_sync_enabled: true,
      sync_config: { repo: "Quazmoz/memoryops" },
      last_sync_at: null,
    };
    fetchMock.mockResolvedValueOnce(jsonResponse(integration));

    await createIntegration(WORKSPACE_ID, {
      source: "github",
      api_token: "  test-github-token  ",
      api_sync_enabled: true,
      sync_config: { repo: "Quazmoz/memoryops" },
    });

    const [, init] = fetchMock.mock.calls[0] as [string, RequestInit];
    expect(JSON.parse(init.body as string)).toEqual({
      source: "github",
      api_token: "test-github-token",
      api_sync_enabled: true,
      sync_config: { repo: "Quazmoz/memoryops" },
    });
  });

  it("rejects creation without a webhook secret or API token and never calls the API", async () => {
    await expect(
      createIntegration(WORKSPACE_ID, { source: "slack", webhook_secret: "   ", api_token: "   " }),
    ).rejects.toThrow("Webhook secret or API token is required");
    expect(fetchMock).not.toHaveBeenCalled();
  });

  it("starts a connector sync with a trimmed repo", async () => {
    fetchMock.mockResolvedValueOnce(jsonResponse({
      source: "github",
      queued_events: 2,
      skipped_events: 0,
      status: "queued",
      message: "Queued 2 GitHub API events from Quazmoz/memoryops; skipped 0.",
    }, 202));

    await startConnectorSync(WORKSPACE_ID, "github", {
      repo: "  Quazmoz/memoryops  ",
      since: "2026-01-01T00:00:00Z",
      limit: 25,
    });

    const [url, init] = fetchMock.mock.calls[0] as [string, RequestInit];
    expect(url).toBe(`/api/v1/workspaces/${WORKSPACE_ID}/integrations/github/sync`);
    expect(init.method).toBe("POST");
    expect(JSON.parse(init.body as string)).toEqual({
      repo: "Quazmoz/memoryops",
      since: "2026-01-01T00:00:00Z",
      limit: 25,
    });
  });

  it("deletes an integration with an encoded source path", async () => {
    fetchMock.mockResolvedValueOnce(new Response(null, { status: 204 }));

    await deleteIntegration(WORKSPACE_ID, "github");

    const [url, init] = fetchMock.mock.calls[0] as [string, RequestInit];
    expect(url).toBe(`/api/v1/workspaces/${WORKSPACE_ID}/integrations/github`);
    expect(init.method).toBe("DELETE");
  });

  it("surfaces backend validation errors", async () => {
    fetchMock.mockResolvedValueOnce(jsonResponse({ detail: "webhook_secret or api_token is required" }, 400));

    await expect(
      createIntegration(WORKSPACE_ID, { source: "jira", webhook_secret: "secret" }),
    ).rejects.toThrow("webhook_secret or api_token is required");
  });
});
