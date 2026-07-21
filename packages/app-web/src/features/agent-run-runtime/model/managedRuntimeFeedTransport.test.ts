import { describe, expect, it } from "vitest";

import { parseLiveEvent } from "./managedRuntimeFeedTransport";

describe("Managed Runtime live transport boundary", () => {
  const canonicalEvent = {
    source: "source-1",
    sequence: "1",
    record: {
      presentation_id: "live:source-1:1",
      presentation: {
        durability: "ephemeral",
        envelope: {
          event: {
            type: "agent_message_delta",
            payload: {
              threadId: "source-1",
              turnId: "turn-1",
              itemId: "item-1",
              delta: "hello",
            },
          },
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
    },
  };

  it("accepts the canonical AgentLiveEvent shape", () => {
    expect(parseLiveEvent(canonicalEvent)).toEqual(canonicalEvent);
  });

  it("rejects the removed provider-telemetry shape", () => {
    expect(
      parseLiveEvent({
        source: "source-1",
        turn_id: "turn-1",
        item_id: "item-1",
        sequence: "1",
        payload: { kind: "text_delta", delta: "hello" },
      }),
    ).toBeNull();
  });
});
