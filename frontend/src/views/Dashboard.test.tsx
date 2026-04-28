import { render, screen } from "@testing-library/react";
import { Database } from "lucide-react";

import type { WorkspaceStats } from "../api/types";
import { MetricCard } from "./Dashboard";

describe("WorkspaceStats", () => {
  it("keeps avg_importance_score mapped on the stats payload", () => {
    const stats = {
      total_memories: 1842,
      episodic_count: 1601,
      semantic_count: 241,
      pinned_count: 14,
      deleted_count: 38,
      avg_importance_score: 0.62,
      avg_decay_score: 0.81,
      memories_created_7d: 94,
      memories_created_30d: 312,
      oldest_memory_at: "2025-11-03T14:22:00Z",
      newest_memory_at: "2026-04-28T09:04:00Z",
    } satisfies WorkspaceStats;

    expect(stats.avg_importance_score).toBe(0.62);
  });
});

describe("MetricCard", () => {
  it("renders a skeleton when loading", () => {
    render(<MetricCard title="Total" value={1842} loading={true} icon={<Database className="h-4 w-4" />} />);

    expect(screen.getByText("Total")).toBeTruthy();
    expect(screen.getByTestId("metric-card-skeleton")).toBeTruthy();
    expect(screen.queryByText("1,842")).toBeNull();
  });
});
