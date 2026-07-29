import { describe, expect, it } from "vitest";

import type {
  BackboneEvent,
  CanonicalConversationRecord,
} from "../../../generated/backbone-protocol";
import { agentRuntimeTestFixtures } from "./agentRuntimeTestFixtures";
import { applyAgentRuntimeUpdate } from "./agentRuntimeProjection";

function record(
  presentationId: string,
  event: BackboneEvent,
): CanonicalConversationRecord {
  return {
    presentation_id: presentationId,
    presentation: {
      durability: "ephemeral",
      envelope: {
        event,
        sessionId: "source-1",
        source: {
          connectorId: "dash-agent",
          connectorType: "native",
          executorId: null,
        },
        trace: { turnId: "turn-1", entryIndex: null },
        observedAt: "2026-07-21T00:00:00Z",
      },
    },
  };
}

describe("Agent Runtime projection", () => {
  it("从 Runtime update 原样接收控制状态并合并 presentation", () => {
    const nameRecord = record("thread-name", {
      type: "thread_name_updated",
      payload: {
        threadId: "source-1",
        threadName: "消息流收束",
      },
    });
    const updated = applyAgentRuntimeUpdate(
      agentRuntimeTestFixtures.snapshots.completed,
      {
        lane_sequence: 3n,
        observation: {
          ...agentRuntimeTestFixtures.snapshots.started.observation,
          revision: 9n,
          thread_name: {
            thread_name: "权威命名",
            source_info: {
              authority: "agent_authoritative",
              source_revision: "source-9",
              fidelity: "exact",
              observed_at_ms: 1000n,
            },
          },
          conversation: [],
        },
        presentations: [nameRecord],
      },
    );

    expect(updated.observation.thread_name?.thread_name).toBe("权威命名");
    expect(updated.observation.execution.active_turn).not.toBeNull();
    expect(updated.observation.conversation).toContainEqual(nameRecord);
  });
});
