import { describe, expect, it } from "vitest";

import type {
  BackboneEvent,
  CanonicalConversationRecord,
} from "../../../generated/backbone-protocol";
import { hasActiveCanonicalTurn } from "./agentLiveProjection";

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

const turn = {
  id: "turn-1",
  items: [],
  itemsView: "full" as const,
  status: "inProgress" as const,
  error: null,
};

describe("canonical Agent execution liveness", () => {
  it("stays active after the first output and ends only at TurnCompleted", () => {
    const started = record("turn-1:start", {
      type: "turn_started",
      payload: { threadId: "source-1", turn },
    });
    const firstOutput = record("turn-1:first-delta", {
      type: "agent_message_delta",
      payload: {
        threadId: "source-1",
        turnId: "turn-1",
        itemId: "item-1",
        delta: "hello",
      },
    });
    const completed = record("turn-1:completed", {
      type: "turn_completed",
      payload: {
        threadId: "source-1",
        turn: { ...turn, status: "completed" },
      },
    });

    expect(hasActiveCanonicalTurn([started, firstOutput])).toBe(true);
    expect(hasActiveCanonicalTurn([started, firstOutput, completed])).toBe(false);
  });
});
