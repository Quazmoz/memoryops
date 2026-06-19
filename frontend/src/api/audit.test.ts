import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import {
  auditExportUrl,
  downloadAuditExport,
  getAuditEntry,
  listAuditActions,
  listAuditEvents,
  verifyAuditChain,
} from "./audit";
import { useAppStore } from "../store/app-store";

const WORKSPACE_ID = "018f0000-0000-7000-8000-000000000001";

const fetchMock = vi.fn();

function jsonResponse(body: unknown, status = 200): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { "content-type": "application/json" },
  });
}

function paramsOf(url: string): URLSearchParams {
  return new URLSearchParams(url.split("?")[1] ?? "");
}

describe("audit api", () => {
  beforeEach(() => {
    vi.stubGlobal("fetch", fetchMock);
    fetchMock.mockReset();
    useAppStore.setState({ workspaceId: WORKSPACE_ID, apiKey: "test-api-key" });
    // jsdom does not implement object URLs; stub them for the download path.
    (URL as unknown as { createObjectURL: () => string }).createObjectURL = vi.fn(() => "blob:mock");
    (URL as unknown as { revokeObjectURL: () => void }).revokeObjectURL = vi.fn();
  });

  afterEach(() => {
    vi.unstubAllGlobals();
    vi.restoreAllMocks();
  });

  it("maps list filters to backend query params and drops a null cursor", async () => {
    fetchMock.mockResolvedValueOnce(jsonResponse({ items: [], limit: 50, offset: 0, next_cursor: null }));

    await listAuditEvents(WORKSPACE_ID, {
      limit: 50,
      cursor: null,
      actions: "key_created,key_revoked",
      category: "api_key",
      severity: "warning",
      success: false,
      actor: "api_key:alpha",
      target_type: "api_key",
      from: "2026-01-01T00:00:00.000Z",
      to: "2026-02-01T00:00:00.000Z",
      q: "secret",
    });

    const [url] = fetchMock.mock.calls[0] as [string];
    expect(url.startsWith(`/api/v1/workspaces/${WORKSPACE_ID}/audit?`)).toBe(true);
    const qs = paramsOf(url);
    expect(qs.get("limit")).toBe("50");
    expect(qs.get("actions")).toBe("key_created,key_revoked");
    expect(qs.get("category")).toBe("api_key");
    expect(qs.get("severity")).toBe("warning");
    expect(qs.get("success")).toBe("false");
    expect(qs.get("actor")).toBe("api_key:alpha");
    expect(qs.get("target_type")).toBe("api_key");
    expect(qs.get("from")).toBe("2026-01-01T00:00:00.000Z");
    expect(qs.get("to")).toBe("2026-02-01T00:00:00.000Z");
    expect(qs.get("q")).toBe("secret");
    // A null cursor must not be sent.
    expect(qs.has("after")).toBe(false);
  });

  it("passes the pagination cursor as the `after` parameter", async () => {
    fetchMock.mockResolvedValueOnce(jsonResponse({ items: [], limit: 50, offset: 0, next_cursor: null }));

    await listAuditEvents(WORKSPACE_ID, { limit: 50, cursor: "2026-01-01T00:00:00Z|018f-cursor" });

    const [url] = fetchMock.mock.calls[0] as [string];
    expect(paramsOf(url).get("after")).toBe("2026-01-01T00:00:00Z|018f-cursor");
  });

  it("verifies the chain via POST", async () => {
    fetchMock.mockResolvedValueOnce(
      jsonResponse({ enabled: true, verified: true, checked: 3, first_broken_seq: null, message: "verified 3 rows" }),
    );

    const result = await verifyAuditChain(WORKSPACE_ID);

    const [url, init] = fetchMock.mock.calls[0] as [string, RequestInit];
    expect(url).toBe(`/api/v1/workspaces/${WORKSPACE_ID}/audit/verify`);
    expect(init.method).toBe("POST");
    expect(result.verified).toBe(true);
    expect(result.checked).toBe(3);
  });

  it("builds an export URL carrying the format and filters", () => {
    const url = auditExportUrl(WORKSPACE_ID, "csv", { category: "security", success: false });
    expect(url.startsWith(`/api/v1/workspaces/${WORKSPACE_ID}/audit/export?`)).toBe(true);
    const qs = paramsOf(url);
    expect(qs.get("format")).toBe("csv");
    expect(qs.get("category")).toBe("security");
    expect(qs.get("success")).toBe("false");
  });

  it("downloads an export with the API key header", async () => {
    fetchMock.mockResolvedValueOnce(
      new Response("occurred_at,id\n", { status: 200, headers: { "content-type": "text/csv" } }),
    );

    await downloadAuditExport(WORKSPACE_ID, "csv", { category: "security" });

    const [url, init] = fetchMock.mock.calls[0] as [string, RequestInit];
    expect(url).toContain(`/v1/workspaces/${WORKSPACE_ID}/audit/export?`);
    expect(paramsOf(url).get("format")).toBe("csv");
    const headers = init.headers as Headers;
    expect(headers.get("x-api-key")).toBe("test-api-key");
  });

  it("throws a descriptive error when the export request fails", async () => {
    fetchMock.mockResolvedValueOnce(new Response("nope", { status: 500 }));
    await expect(downloadAuditExport(WORKSPACE_ID, "jsonl", {})).rejects.toThrow("Export failed (500)");
  });

  it("fetches a single entry and the actions catalog at the expected paths", async () => {
    fetchMock.mockResolvedValueOnce(jsonResponse({ id: "audit-1", action: "key_created" }));
    await getAuditEntry(WORKSPACE_ID, "audit-1");
    expect((fetchMock.mock.calls[0] as [string])[0]).toBe(
      `/api/v1/workspaces/${WORKSPACE_ID}/audit/audit-1`,
    );

    fetchMock.mockResolvedValueOnce(jsonResponse({ actions: [], severities: ["info"], categories: [] }));
    await listAuditActions(WORKSPACE_ID);
    expect((fetchMock.mock.calls[1] as [string])[0]).toBe(
      `/api/v1/workspaces/${WORKSPACE_ID}/audit/actions`,
    );
  });
});
