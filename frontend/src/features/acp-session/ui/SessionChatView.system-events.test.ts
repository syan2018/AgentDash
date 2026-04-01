import { describe, expect, it } from "vitest";
import type { SessionNotification } from "@agentclientprotocol/sdk";

import { collectNewSystemEvents } from "./SessionChatView";
import type { SessionEventEnvelope } from "../model/types";

function buildEvent(eventSeq: number, eventType: string | null): SessionEventEnvelope {
  return {
    session_id: "sess-1",
    event_seq: eventSeq,
    occurred_at_ms: eventSeq,
    committed_at_ms: eventSeq,
    notification: {
      sessionId: "sess-1",
      update: eventType == null
        ? {
            sessionUpdate: "tool_call",
            toolCallId: "tool-1",
            title: "执行 shell",
            kind: "execute",
            status: "pending",
            content: [],
          }
        : {
            sessionUpdate: "session_info_update",
            _meta: {
              agentdash: {
                v: 1,
                trace: {
                  turnId: "turn-1",
                },
                event: {
                  type: eventType,
                },
              },
            },
          },
    } as unknown as SessionNotification,
  };
}

describe("collectNewSystemEvents", () => {
  it("按 event_seq 只返回尚未处理过的 system events", () => {
    const rawEvents = [
      buildEvent(2, null),
      buildEvent(3, "turn_started"),
      buildEvent(4, "turn_completed"),
    ];

    const first = collectNewSystemEvents(rawEvents, 2);
    expect(first.items.map((item) => item.eventType)).toEqual([
      "turn_started",
      "turn_completed",
    ]);
    expect(first.lastSeenSeq).toBe(4);

    const second = collectNewSystemEvents(rawEvents, first.lastSeenSeq);
    expect(second.items).toHaveLength(0);
    expect(second.lastSeenSeq).toBe(4);
  });
});
