import { describe, expect, it } from "vitest";

import type {
  AgentRunView,
  LifecycleAgentExecutionView,
  LifecycleRunView,
} from "../generated/workflow-contracts";
import { lifecycleRuntimeTraceSummaries } from "./lifecycle-views";

function agent(agentId: string): AgentRunView {
  return {
    agent_ref: { run_id: "run-1", agent_id: agentId },
    project_id: "project-1",
    source: "workflow_agent",
    status: "active",
    created_at: "2026-07-19T00:00:00Z",
    updated_at: "2026-07-19T00:00:00Z",
  };
}

function lifecycleRun(agents: LifecycleAgentExecutionView[]): LifecycleRunView {
  return {
    run_ref: { run_id: "run-1" },
    project_id: "project-1",
    topology: "workflow_graph",
    status: "running",
    orchestrations: [],
    active_runtime_node_refs: [],
    agents,
    subject_associations: [],
    execution_log: [],
    created_at: "2026-07-19T00:00:00Z",
    updated_at: "2026-07-19T00:00:00Z",
    last_activity_at: "2026-07-19T00:00:00Z",
  };
}

describe("lifecycleRuntimeTraceSummaries", () => {
  it("preserves typed absence reasons per AgentRun", () => {
    const view = lifecycleRun([
      {
        agent: agent("agent-absent"),
        runtime: {
          state: "absent",
          target: { run_id: "run-1", agent_id: "agent-absent" },
          reason: "product_binding_missing",
        },
        attempts: [],
      },
      {
        agent: agent("agent-unavailable"),
        runtime: {
          state: "absent",
          target: { run_id: "run-1", agent_id: "agent-unavailable" },
          reason: "agent_unavailable",
        },
        attempts: [],
      },
    ]);

    expect(lifecycleRuntimeTraceSummaries(view)).toEqual([
      {
        agent: expect.objectContaining({
          agent_ref: { run_id: "run-1", agent_id: "agent-absent" },
        }),
        state: "absent",
        runtimeThreadId: null,
        reason: "product_binding_missing",
      },
      {
        agent: expect.objectContaining({
          agent_ref: { run_id: "run-1", agent_id: "agent-unavailable" },
        }),
        state: "absent",
        runtimeThreadId: null,
        reason: "agent_unavailable",
      },
    ]);
  });
});
