import { describe, expect, it } from "vitest";

import { parseAgentRuntimeUpdate } from "./agentRuntimeUpdateTransport";

describe("Agent Runtime update transport boundary", () => {
  const update = {
    lane_sequence: "1",
    observation: {
      revision: "2",
      lifecycle: "active",
      execution: {
        active_turn: {
          turn_id: "turn-1",
          kind: "conversation",
          phase: "running",
          started_at_ms: "1000",
          cancellable: true,
        },
        queued_compaction: null,
        last_compaction_outcome: null,
      },
      context: {
        snapshot_revision: "2",
        context_revision: "context-2",
        recipe_digest: "sha256:context-2",
        authority: "agent_owned",
        fidelity: "exact",
      },
      command_availability: {},
      interactions: [],
      thread_name: null,
      source_info: {
        authority: "agent_authoritative",
        source_revision: "source-2",
        fidelity: "exact",
        observed_at_ms: "1000",
      },
      conversation: [],
    },
    presentations: [],
  };

  it("接受包含控制事实的 AgentRuntimeUpdate", () => {
    expect(parseAgentRuntimeUpdate(update)).toEqual({
      ...update,
      lane_sequence: 1n,
      observation: {
        ...update.observation,
        revision: 2n,
        execution: {
          ...update.observation.execution,
          active_turn: {
            ...update.observation.execution.active_turn,
            started_at_ms: 1000n,
          },
        },
        context: {
          ...update.observation.context,
          snapshot_revision: 2n,
        },
        source_info: {
          ...update.observation.source_info,
          observed_at_ms: 1000n,
        },
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
