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
  it("连续 update 保留当前 lane 已归约的流式增量", () => {
    const firstDelta = record("assistant-delta-1", {
      type: "agent_message_delta",
      payload: {
        threadId: "source-1",
        turnId: "turn-1",
        itemId: "assistant-1",
        delta: "第一段",
      },
    });
    const secondDelta = record("assistant-delta-2", {
      type: "agent_message_delta",
      payload: {
        threadId: "source-1",
        turnId: "turn-1",
        itemId: "assistant-1",
        delta: "第二段",
      },
    });
    const firstView = applyAgentRuntimeUpdate(
      agentRuntimeTestFixtures.snapshots.completed,
      {
        lane_sequence: 1n,
        observation: {
          ...agentRuntimeTestFixtures.snapshots.started.observation,
          conversation: [],
        },
        presentations: [firstDelta],
      },
    );

    const secondView = applyAgentRuntimeUpdate(firstView, {
      lane_sequence: 2n,
      observation: {
        ...agentRuntimeTestFixtures.snapshots.started.observation,
        revision: agentRuntimeTestFixtures.snapshots.started.observation.revision + 1n,
        conversation: [],
      },
      presentations: [secondDelta],
    });

    expect(secondView.observation.conversation).toEqual([
      firstDelta,
      secondDelta,
    ]);
  });

  it("连续 update 不丢失 file edit patch 过程记录", () => {
    const patch = (presentationId: string, diff: string) =>
      record(presentationId, {
        type: "file_change_patch_updated",
        payload: {
          threadId: "source-1",
          turnId: "turn-1",
          itemId: "file-edit-1",
          changes: [{
            path: "src/main.ts",
            kind: { type: "update", move_path: null },
            diff,
          }],
        },
      });
    const firstPatch = patch("file-edit-patch-1", "-old\n+first");
    const secondPatch = patch("file-edit-patch-2", "-old\n+second");
    const firstView = applyAgentRuntimeUpdate(
      agentRuntimeTestFixtures.snapshots.completed,
      {
        lane_sequence: 1n,
        observation: {
          ...agentRuntimeTestFixtures.snapshots.started.observation,
          conversation: [],
        },
        presentations: [firstPatch],
      },
    );

    const secondView = applyAgentRuntimeUpdate(firstView, {
      lane_sequence: 2n,
      observation: {
        ...agentRuntimeTestFixtures.snapshots.started.observation,
        revision: agentRuntimeTestFixtures.snapshots.started.observation.revision + 1n,
        conversation: [],
      },
      presentations: [secondPatch],
    });

    expect(secondView.observation.conversation).toEqual([
      firstPatch,
      secondPatch,
    ]);
  });

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
