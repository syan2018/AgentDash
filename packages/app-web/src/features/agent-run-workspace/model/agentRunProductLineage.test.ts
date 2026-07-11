import { describe, expect, it } from "vitest";
import type {
  AgentRunProductLineageAgentView,
  AgentRunProductLineageView,
} from "../../../generated/workflow-contracts";
import { collectAgentRunProductCompanionRefs } from "./agentRunProductLineage";

function lineageAgent(
  agentId: string,
  children: AgentRunProductLineageAgentView[] = [],
): AgentRunProductLineageAgentView {
  return {
    run_ref: { run_id: "run-1" },
    agent_ref: { run_id: "run-1", agent_id: agentId },
    title: `Agent ${agentId}`,
    lifecycle_status: "active",
    last_activity_at: "2026-07-12T00:00:00Z",
    runtime: {
      thread_status: agentId === "child" ? "active" : "closed",
      active_turn_id: agentId === "child" ? "turn-1" : undefined,
    },
    children,
  };
}

describe("collectAgentRunProductCompanionRefs", () => {
  it("projects recursive detail lineage without including the parent", () => {
    const lineage: AgentRunProductLineageView = {
      parent: lineageAgent("parent"),
      children: [lineageAgent("child", [lineageAgent("grandchild")])],
    };

    expect(collectAgentRunProductCompanionRefs(lineage)).toEqual([
      {
        run_id: "run-1",
        agent_id: "child",
        display_title: "Agent child",
        delivery_status: "running",
        last_activity_at: "2026-07-12T00:00:00Z",
      },
      {
        run_id: "run-1",
        agent_id: "grandchild",
        display_title: "Agent grandchild",
        delivery_status: "idle",
        last_activity_at: "2026-07-12T00:00:00Z",
      },
    ]);
  });

  it("returns no companion refs while the product projection is unavailable", () => {
    expect(collectAgentRunProductCompanionRefs(null)).toEqual([]);
  });
});
