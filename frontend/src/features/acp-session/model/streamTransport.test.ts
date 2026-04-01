import { describe, expect, it } from "vitest";
import type { SessionNotification } from "@agentclientprotocol/sdk";

import { parseSessionEventEnvelopePayload } from "./streamTransport";

function buildToolCallNotification(): SessionNotification {
  return {
    sessionId: "sess-1",
    update: {
      sessionUpdate: "tool_call",
      toolCallId: "tool-1",
      title: "写文件",
      kind: "edit",
      status: "pending",
      content: [],
      rawInput: {
        path: "README.md",
        content: "hello",
      },
    },
  } as unknown as SessionNotification;
}

describe("parseSessionEventEnvelopePayload", () => {
  it("会保留实时 envelope 上的 trace 与 tool_call 元信息", () => {
    const notification = buildToolCallNotification();

    const parsed = parseSessionEventEnvelopePayload({
      type: "event",
      session_id: "sess-1",
      event_seq: 12,
      occurred_at_ms: 1000,
      committed_at_ms: 1005,
      session_update_type: "tool_call",
      turn_id: "turn-1",
      entry_index: 3,
      tool_call_id: "tool-1",
      notification,
    });

    expect(parsed).toEqual({
      session_id: "sess-1",
      event_seq: 12,
      notification,
      occurred_at_ms: 1000,
      committed_at_ms: 1005,
      session_update_type: "tool_call",
      turn_id: "turn-1",
      entry_index: 3,
      tool_call_id: "tool-1",
    });
  });
});
