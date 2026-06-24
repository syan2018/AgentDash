import { describe, expect, it } from "vitest";
import type { BackboneEnvelope } from "../../../generated/backbone-protocol";
import { parseSessionEventEnvelopePayload } from "./streamTransport";

function envelope(): BackboneEnvelope {
  return {
    sessionId: "s1",
    source: {
      connectorId: "connector",
      connectorType: "test",
      executorId: null,
    },
    trace: {
      turnId: "turn-1",
      entryIndex: 0,
    },
    observedAt: "2026-06-11T00:00:00.000Z",
    event: {
      type: "agent_message_delta",
      payload: {
        threadId: "thread-1",
        turnId: "turn-1",
        itemId: "item-1",
        delta: "hello",
      },
    },
  };
}

describe("parseSessionEventEnvelopePayload", () => {
  it("只接受 generated event envelope 的字段", () => {
    const result = parseSessionEventEnvelopePayload({
      type: "event",
      session_id: "s1",
      event_seq: 7,
      occurred_at_ms: 10,
      committed_at_ms: 11,
      session_update_type: "agent_message_delta",
      turn_id: "turn-1",
      entry_index: 0,
      tool_call_id: undefined,
      notification: envelope(),
    });

    expect(result.error).toBeNull();
    expect(result.event?.event_seq).toBe(7);
    expect(result.event?.notification.event.type).toBe("agent_message_delta");
    expect(result.event?.ephemeral).toBe(false);
  });

  it("ephemeral_event envelope 解析为带 ephemeral=true 标记", () => {
    const result = parseSessionEventEnvelopePayload({
      type: "ephemeral_event",
      session_id: "s1",
      event_seq: 0,
      occurred_at_ms: 10,
      committed_at_ms: 11,
      session_update_type: "agent_message_delta",
      turn_id: "turn-1",
      entry_index: 0,
      tool_call_id: undefined,
      notification: envelope(),
    });

    expect(result.error).toBeNull();
    expect(result.event?.ephemeral).toBe(true);
    expect(result.event?.event_seq).toBe(0);
  });

  it("缺少 event_seq 时返回错误且不读取旧 id fallback", () => {
    const result = parseSessionEventEnvelopePayload({
      type: "event",
      session_id: "s1",
      id: 99,
      occurred_at_ms: 10,
      committed_at_ms: 11,
      session_update_type: "agent_message_delta",
      notification: envelope(),
    });

    expect(result.event).toBeNull();
    expect(result.error?.message).toContain("event_seq");
  });

  it("缺少 notification 时返回错误并丢弃", () => {
    const result = parseSessionEventEnvelopePayload({
      type: "event",
      session_id: "s1",
      event_seq: 8,
      occurred_at_ms: 10,
      committed_at_ms: 11,
      session_update_type: "agent_message_delta",
    });

    expect(result.event).toBeNull();
    expect(result.error?.message).toContain("notification");
  });
});
