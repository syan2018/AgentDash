import { describe, expect, it } from "vitest";

import type {
  AgentRunView,
  LifecycleAgentExecutionView,
  LifecycleRunView,
} from "../generated/workflow-contracts";
import type { RuntimeU64 } from "../generated/agent-runtime-contracts";
import { lifecycleRuntimeTraceSummaries } from "./lifecycle-views";

const runtimeU64 = (value: number): RuntimeU64 => String(value) as RuntimeU64;

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
  it("preserves typed absent and stale evidence per AgentRun", () => {
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
        agent: agent("agent-stale"),
        runtime: {
          state: "stale",
          reason: "runtime_applied_surface_mismatch",
          evidence: {
            expected_target: { run_id: "run-1", agent_id: "agent-stale" },
            observed_target: { run_id: "run-1", agent_id: "agent-stale" },
            expected_runtime_thread_id: "thread-expected",
            observed_runtime_thread_id: "thread-stale",
            observed_source_binding: {
              source_ref: "source-stale",
              committed_at_revision: runtimeU64(7),
              applied_surface_revision: runtimeU64(5),
              activated_at_revision: runtimeU64(7),
            },
            observed_snapshot: null,
          },
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
          agent_ref: { run_id: "run-1", agent_id: "agent-stale" },
        }),
        state: "stale",
        runtimeThreadId: "thread-stale",
        reason: "runtime_source_binding_mismatch",
      },
    ]);
  });
});
