import { describe, expect, it } from "vitest";

import type { SessionDisplayEntry, SessionEventEnvelope } from "./types";
import { segmentByTurn } from "./useSessionFeed";

function failedTurnEvent(): SessionEventEnvelope {
  return {
    session_id: "session-1",
    event_seq: 1,
    occurred_at_ms: 0,
    committed_at_ms: 0,
    session_update_type: "turn_completed",
    turn_id: "turn-1",
    entry_index: 3,
    tool_call_id: null,
    notification: {
      event: {
        type: "turn_completed",
        payload: {
          threadId: "session-1",
          turn: {
            id: "turn-1",
            items: [],
            itemsView: "full",
            status: "failed",
            error: {
              message: "provider rejected reasoning effort",
              additionalDetails: "code=unsupported_value; retryable=false",
            },
          },
        },
      },
      sessionId: "session-1",
      source: {
        connectorId: "dash-agent",
        connectorType: "native",
        executorId: null,
      },
      trace: { turnId: "turn-1", entryIndex: 3 },
      observedAt: "1970-01-01T00:00:00.000Z",
    },
    presentation_id: "turn-1:terminal",
    runtime_change_sequence: null,
    baseline: true,
  };
}

describe("session turn segmentation", () => {
  it("keeps a failed terminal-only turn and its authoritative error", () => {
    const segments = segmentByTurn([], [failedTurnEvent()], null);

    expect(segments).toEqual([
      expect.objectContaining({
        turnId: "turn-1",
        status: "failed",
        errorMessage: "provider rejected reasoning effort",
        items: [],
      }),
    ]);
  });

  it("keeps a durable completed agent message as the collapsed turn output", () => {
    const agentMessage: SessionDisplayEntry = {
      id: "item:assistant-1",
      sessionId: "session-1",
      timestamp: 2,
      eventSeq: 2,
      turnId: "turn-1",
      itemFreshness: "completed",
      isStreaming: false,
      event: {
        type: "item_completed",
        payload: {
          threadId: "session-1",
          turnId: "turn-1",
          completedAtMs: 2,
          item: {
            type: "agentMessage",
            id: "assistant-1",
            text: "最终回答",
          },
        },
      },
    };

    const [segment] = segmentByTurn([agentMessage], [], null);

    expect(segment?.finalOutput).toBe(agentMessage);
  });
});
