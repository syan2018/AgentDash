import { describe, expect, it } from "vitest";

import { parseAgentRuntimeUpdate } from "./agentRuntimeUpdateTransport";

describe("Agent Runtime update transport boundary", () => {
  const update = {
    lane_sequence: "1",
    view_revision: "2",
    execution: {
      status: "active",
      active_turn: {
        turn_id: "turn-1",
        kind: "conversation",
        phase: "running",
        started_at_ms: "1000",
        cancellable: true,
      },
      queued_compaction: null,
      last_compaction_outcome: null,
      latest_turn_id: "turn-1",
    },
    context: {
      snapshot_revision: "2",
      context_revision: "context-2",
      recipe_digest: "sha256:context-2",
      authority: "source_authoritative",
      fidelity: "exact",
    },
    command_availability: {},
    interactions: [],
    presentations: [],
  };

  it("接受包含控制事实的 AgentRuntimeUpdate", () => {
    expect(parseAgentRuntimeUpdate(update)).toEqual({
      ...update,
      lane_sequence: 1n,
      view_revision: 2n,
      execution: {
        ...update.execution,
        active_turn: {
          ...update.execution.active_turn,
          started_at_ms: 1000n,
        },
      },
      context: {
        ...update.context,
        snapshot_revision: 2n,
      },
    });
  });

  it("拒绝旧 AgentLiveEvent 形状", () => {
    expect(
      parseAgentRuntimeUpdate({
        source: "source-1",
        sequence: "1",
        record: {},
      }),
    ).toBeNull();
  });
});
