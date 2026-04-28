import { buildSearchRequest } from "./memory";

describe("buildSearchRequest", () => {
  it("builds hybrid search filters from explorer state", () => {
    const request = buildSearchRequest("018f0000-0000-7000-8000-000000000001", {
      query: "  mona retrieval notes  ",
      memoryType: "semantic",
      pinned: true,
      minImportance: 0.55,
      tags: ["retrieval", "github"],
      limit: 25,
      offset: 10,
    });

    expect(request).toEqual({
      query: "mona retrieval notes",
      workspace_id: "018f0000-0000-7000-8000-000000000001",
      mode: "hybrid",
      limit: 25,
      offset: 10,
      memory_types: ["semantic"],
      filters: {
        memory_type: "semantic",
        pinned: true,
        min_importance: 0.55,
        tags: ["retrieval", "github"],
      },
    });
  });

  it("omits empty filters for broad searches", () => {
    const request = buildSearchRequest("018f0000-0000-7000-8000-000000000001", {
      query: "memory",
      memoryType: "all",
      pinned: false,
      minImportance: 0,
      tags: [],
      limit: 50,
      offset: 0,
    });

    expect(request.filters).toBeUndefined();
    expect(request.mode).toBe("hybrid");
  });

  it("places scope filters on the top-level search request", () => {
    const request = buildSearchRequest("018f0000-0000-7000-8000-000000000001", {
      query: "memory",
      memoryType: "all",
      pinned: false,
      minImportance: 0,
      tags: [],
      agentId: "agent-1",
      userId: "user-1",
      repo: "Quazmoz/memoryops",
      limit: 50,
      offset: 0,
    });

    expect(request.agent_id).toBe("agent-1");
    expect(request.user_id).toBe("user-1");
    expect(request.repo).toBe("Quazmoz/memoryops");
    expect(request.filters).toBeUndefined();
  });
});
