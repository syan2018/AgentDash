import { describe, expect, it } from "vitest";
import type { JsonValue } from "../../../generated/common-contracts";
import type { BackboneEvent, Turn } from "../../../generated/backbone-protocol";
import type { SessionEventEnvelope } from "../model/types";
import { computeProjectionRefreshKey } from "./SessionChatView";

const completedTurn: Turn = {
  id: "turn-1",
  items: [],
  itemsView: "full",
  status: "completed",
  error: null,
  startedAt: null,
  completedAt: null,
  durationMs: null,
};

function eventEnvelope(eventSeq: number, event: BackboneEvent): SessionEventEnvelope {
  return {
    session_id: "session-1",
    event_seq: eventSeq,
    occurred_at_ms: eventSeq,
    committed_at_ms: eventSeq,
    session_update_type: event.type,
    notification: {
      event,
      sessionId: "session-1",
      source: {
        connectorId: "test",
        connectorType: "unit",
        executorId: null,
      },
      trace: {
        turnId: null,
        entryIndex: null,
      },
      observedAt: "2026-05-26T00:00:00.000Z",
    },
  };
}

function agentDeltaEvent(itemId: string): BackboneEvent {
  return {
    type: "agent_message_delta",
    payload: {
      threadId: "thread-1",
      turnId: "turn-1",
      itemId,
      delta: "delta",
    },
  };
}

function platformMetaEvent(key: string, value: Record<string, JsonValue>): BackboneEvent {
  return {
    type: "platform",
    payload: {
      kind: "session_meta_update",
      data: { key, value },
    },
  };
}

describe("computeProjectionRefreshKey", () => {
  it("普通 delta event 不推进 projection refresh key", () => {
    const events = [
      eventEnvelope(1, {
        type: "turn_completed",
        payload: { threadId: "thread-1", turn: completedTurn },
      }),
      eventEnvelope(2, agentDeltaEvent("assistant-1")),
      eventEnvelope(3, agentDeltaEvent("assistant-1")),
    ];

    expect(computeProjectionRefreshKey(events)).toBe(1);
  });

  it("外部 executor_context_compacted 不推进 projection refresh key", () => {
    const events = [
      eventEnvelope(1, agentDeltaEvent("assistant-1")),
      eventEnvelope(2, {
        type: "executor_context_compacted",
        payload: { threadId: "thread-1", turnId: "turn-1" },
      }),
      eventEnvelope(3, agentDeltaEvent("assistant-2")),
    ];

    expect(computeProjectionRefreshKey(events)).toBe(0);
  });

  it("compaction_summary context_frame 会推进 projection refresh key", () => {
    const events = [
      eventEnvelope(1, agentDeltaEvent("assistant-1")),
      eventEnvelope(3, platformMetaEvent("context_frame", {
        kind: "compaction_summary",
        id: "frame-1",
      })),
      eventEnvelope(4, agentDeltaEvent("assistant-2")),
    ];

    expect(computeProjectionRefreshKey(events)).toBe(3);
  });

  it("platform context_compacted meta event 会推进 projection refresh key", () => {
    const events = [
      eventEnvelope(1, agentDeltaEvent("assistant-1")),
      eventEnvelope(2, platformMetaEvent("context_compacted", {
        summary: "历史摘要",
        messages_compacted: 2,
      })),
    ];

    expect(computeProjectionRefreshKey(events)).toBe(2);
  });
});
