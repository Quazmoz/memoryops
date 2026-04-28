import { render, screen } from "@testing-library/react";
import { vi } from "vitest";

import type { RetrieveResponse, RetrievalTrace } from "../api/types";
import { useRetrieve, useRetrievalTrace } from "../hooks/use-workspace";
import { RetrievalTraceView } from "./RetrievalTraceView";

vi.mock("../hooks/use-workspace", () => ({
  useRetrieve: vi.fn(),
  useRetrievalTrace: vi.fn(),
}));

const mockUseRetrieve = vi.mocked(useRetrieve);
const mockUseRetrievalTrace = vi.mocked(useRetrievalTrace);

describe("RetrievalTraceView", () => {
  beforeEach(() => {
    mockUseRetrieve.mockReturnValue(mutationState());
    mockUseRetrievalTrace.mockReturnValue(queryState());
  });

  it("disables the submit button while pending", () => {
    mockUseRetrieve.mockReturnValue(mutationState({ isPending: true }));

    render(<RetrievalTraceView />);

    expect((screen.getByTestId("trace-submit") as HTMLButtonElement).disabled).toBe(true);
  });

  it("shows the summary bar after a successful mutation", () => {
    mockUseRetrieve.mockReturnValue(mutationState({ data: retrieveResponse() }));

    render(<RetrievalTraceView />);

    expect(screen.getByTestId("trace-summary")).toBeTruthy();
  });

  it("renders the candidates table", () => {
    mockUseRetrievalTrace.mockReturnValue(queryState({ data: retrievalTrace() }));

    render(<RetrievalTraceView initialActiveQueryId="query-1" />);

    expect(screen.getByTestId("trace-candidates-table")).toBeTruthy();
  });
});

function mutationState(overrides: Record<string, unknown> = {}) {
  return {
    data: undefined,
    error: null,
    isError: false,
    isPending: false,
    mutate: vi.fn(),
    ...overrides,
  } as unknown as ReturnType<typeof useRetrieve>;
}

function queryState(overrides: Record<string, unknown> = {}) {
  return {
    data: undefined,
    error: null,
    isError: false,
    isLoading: false,
    ...overrides,
  } as unknown as ReturnType<typeof useRetrievalTrace>;
}

function retrieveResponse(): RetrieveResponse {
  return {
    query_id: "query-1",
    items: [0, 1, 2].map((index) => ({
      memory_id: `memory-${index}`,
      content: `Packed memory ${index}`,
      memory_type: index === 1 ? "semantic" : "episodic",
      importance_score: 0.82,
      decay_score: 0.91,
      rrf_score: 0.62,
      token_count: 18,
      tags: ["trace"],
      created_at: "2026-04-28T12:00:00Z",
    })),
    total_candidates: 8,
    token_count: 54,
    elapsed_ms: 24,
  };
}

function retrievalTrace(): RetrievalTrace {
  return {
    query_id: "query-1",
    query_text: "deployment rollout",
    search_mode: "hybrid",
    created_at: "2026-04-28T12:00:00Z",
    elapsed_ms: 19,
    total_candidates: 2,
    included_count: 1,
    token_budget: 4000,
    token_count: 28,
    candidates: [
      {
        memory_id: "abcdef0123456789",
        content_snippet: "Included memory",
        memory_type: "episodic",
        keyword_score: 0.51,
        vector_score: 0.73,
        rrf_score: 0.61,
        decay_score: 0.92,
        importance_score: 0.88,
        final_score: 0.77,
        included: true,
        exclusion_reason: null,
        token_count: 12,
      },
      {
        memory_id: "fedcba9876543210",
        content_snippet: "Excluded memory",
        memory_type: "semantic",
        keyword_score: 0.21,
        vector_score: null,
        rrf_score: 0.24,
        decay_score: 0.38,
        importance_score: 0.44,
        final_score: 0.31,
        included: false,
        exclusion_reason: "token_budget_exceeded",
        token_count: 16,
      },
    ],
  };
}
