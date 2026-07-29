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
        view_revision: 9n,
        execution: {
          status: "active",
          active_turn: {
            turn_id: "turn-1",
            kind: "conversation",
            phase: "running",
            started_at_ms: 1000n,
            cancellable: true,
          },
          queued_compaction: null,
          last_compaction_outcome: null,
          latest_turn_id: "turn-1",
        },
        context: {
          snapshot_revision: 9n,
          context_revision: "context-9",
          recipe_digest: "sha256:context-9",
          authority: "source_authoritative",
          fidelity: "exact",
        },
        command_availability:
          agentRuntimeTestFixtures.snapshots.started.command_availability,
        interactions: [],
        presentations: [nameRecord],
      },
    );

    expect(updated.thread_name).toBe("消息流收束");
    expect(updated.execution.status).toBe("active");
    expect(updated.conversation).toContainEqual(nameRecord);
  });
});
