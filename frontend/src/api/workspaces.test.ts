import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { deleteWorkspace, listWorkspaces, normalizeWorkspaceList } from "./workspaces";
import { useAppStore } from "../store/app-store";

const WORKSPACE_ID = "018f0000-0000-7000-8000-000000000001";

const fetchMock = vi.fn();

function jsonResponse(body: unknown, status = 200): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { "content-type": "application/json" },
  });
}

describe("normalizeWorkspaceList", () => {
  it("consumes the real list endpoint shape", () => {
    const payload = {
      workspaces: [
        { id: "a", name: "Alpha", created_at: "2026-01-01T00:00:00Z" },
        { id: "b", name: "Beta", created_at: "2026-02-01T00:00:00Z" },
      ],
    };

    expect(normalizeWorkspaceList(payload)).toEqual(payload.workspaces);
  });

  it("normalizes the legacy single-workspace shape", () => {
    expect(normalizeWorkspaceList({ id: "a", name: "Alpha" })).toEqual([
      { id: "a", name: "Alpha", created_at: "" },
    ]);
  });

  it("returns an empty list for unrecognized payloads", () => {
    expect(normalizeWorkspaceList(null)).toEqual([]);
    expect(normalizeWorkspaceList({ workspaces: "nope" })).toEqual([]);
    expect(normalizeWorkspaceList("garbage")).toEqual([]);
  });
});

describe("workspaces api", () => {
  beforeEach(() => {
    vi.stubGlobal("fetch", fetchMock);
    fetchMock.mockReset();
    useAppStore.setState({ workspaceId: WORKSPACE_ID, apiKey: "test-store-key" });
  });

  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("lists workspaces from GET /v1/workspaces using the supplied key", async () => {
    fetchMock.mockResolvedValueOnce(
      jsonResponse({ workspaces: [{ id: "a", name: "Alpha", created_at: "2026-01-01T00:00:00Z" }] }),
    );

    const workspaces = await listWorkspaces("test-explicit-key");

    expect(workspaces).toEqual([{ id: "a", name: "Alpha", created_at: "2026-01-01T00:00:00Z" }]);
    const [url, init] = fetchMock.mock.calls[0] as [string, RequestInit];
    expect(url).toBe("/api/v1/workspaces");
    expect(init.method).toBe("GET");
    expect(new Headers(init.headers).get("x-api-key")).toBe("test-explicit-key");
  });

  it("returns an empty list without a network call when the key is blank", async () => {
    await expect(listWorkspaces("   ")).resolves.toEqual([]);
    expect(fetchMock).not.toHaveBeenCalled();
  });

  it("deletes a workspace and parses the deleted flag", async () => {
    fetchMock.mockResolvedValueOnce(jsonResponse({ deleted: true }));

    await expect(deleteWorkspace(WORKSPACE_ID)).resolves.toEqual({ deleted: true });
    const [url, init] = fetchMock.mock.calls[0] as [string, RequestInit];
    expect(url).toBe(`/api/v1/workspaces/${WORKSPACE_ID}`);
    expect(init.method).toBe("DELETE");
  });

  it("tolerates an empty 204 delete response", async () => {
    fetchMock.mockResolvedValueOnce(new Response(null, { status: 204 }));

    await expect(deleteWorkspace(WORKSPACE_ID)).resolves.toEqual({ deleted: true });
  });
});
