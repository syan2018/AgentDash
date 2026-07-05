import { describe, expect, it } from "vitest";

import type { BackboneEvent } from "../../../generated/backbone-protocol";
import type { SessionEventEnvelope } from "./types";
import { agentRunSeedEntries, agentRunSyntheticSessionId } from "./agentRunStreamIdentity";

function envelope(eventSeq: number, event: BackboneEvent): SessionEventEnvelope {
  return {
    session_id: "parent-session",
    event_seq: eventSeq,
    occurred_at_ms: eventSeq,
    committed_at_ms: eventSeq,
    session_update_type: event.type,
    turn_id: "turn-1",
    entry_index: eventSeq,
    notification: {
      event,
      sessionId: "parent-session",
      source: {
        connectorId: "test",
        connectorType: "unit",
        executorId: null,
      },
      trace: {
        turnId: "turn-1",
        entryIndex: eventSeq,
      },
      observedAt: "2026-07-05T00:00:00.000Z",
    },
  };
}

describe("agentRunSeedEntries", () => {
  it("preserves native tool item types from parent replay events", () => {
    const target = { runId: "run-1", agentId: "agent-1" };
    const entries = agentRunSeedEntries([
      envelope(8, {
        type: "item_completed",
        payload: {
          threadId: "parent-thread",
          turnId: "turn-1",
          completedAtMs: 8,
          item: {
            type: "fsGlob",
            id: "glob-1",
            pattern: "*.rs",
            path: null,
            maxResults: null,
            arguments: { pattern: "*.rs" },
            status: "completed",
            contentItems: null,
            success: true,
          },
        },
      }),
    ], target);

    expect(entries).toHaveLength(1);
    expect(entries[0]?.sessionId).toBe(agentRunSyntheticSessionId(target));
    expect(entries[0]?.eventSeq).toBeLessThan(0);
    const event = entries[0]?.event;
    expect(event?.type).toBe("item_completed");
    if (event?.type !== "item_completed") throw new Error("expected item_completed");
    expect(event.payload.item.type).toBe("fsGlob");
  });
});
