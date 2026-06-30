import { describe, expect, it } from "vitest";
import type { BackboneEnvelope } from "../../../generated/backbone-protocol";
import {
  parseSessionEventEnvelopePayload,
  parseSessionNdjsonEnvelope,
} from "./sessionNdjsonEnvelopeValidator";

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

describe("parseSessionNdjsonEnvelope", () => {
  it("接受合法 connected envelope", () => {
    const result = parseSessionNdjsonEnvelope({
      type: "connected",
      last_event_id: 12,
      ephemeral_epoch: 3,
    });

    if (!result.ok) {
      throw result.error;
    }
    if (result.kind !== "connected") {
      throw new Error(`Expected connected envelope, got ${result.kind}`);
    }
    expect(result.envelope.last_event_id).toBe(12);
    expect(result.envelope.ephemeral_epoch).toBe(3);
  });

  it("接受合法 event envelope", () => {
    const result = parseSessionNdjsonEnvelope({
      type: "event",
      session_id: "s1",
      event_seq: 7,
      occurred_at_ms: 10,
      committed_at_ms: 11,
      session_update_type: "agent_message_delta",
      notification: envelope(),
    });

    if (!result.ok) {
      throw result.error;
    }
    if (result.kind !== "event") {
      throw new Error(`Expected event envelope, got ${result.kind}`);
    }
    expect(result.envelope.session_id).toBe("s1");
    expect(result.envelope.notification.event.type).toBe("agent_message_delta");
  });

  it("接受合法 heartbeat envelope", () => {
    const result = parseSessionNdjsonEnvelope({
      type: "heartbeat",
      timestamp: 100,
    });

    if (!result.ok) {
      throw result.error;
    }
    if (result.kind !== "heartbeat") {
      throw new Error(`Expected heartbeat envelope, got ${result.kind}`);
    }
    expect(result.envelope.timestamp).toBe(100);
  });

  it("拒绝 invalid session-shape", () => {
    const result = parseSessionNdjsonEnvelope({
      type: "event",
      session_id: "s1",
      event_seq: "7",
      occurred_at_ms: 10,
      committed_at_ms: 11,
      session_update_type: "agent_message_delta",
      notification: envelope(),
    });

    expect(result.ok).toBe(false);
    if (!result.ok) {
      expect(result.error.message).toContain("event_seq");
    }
  });

  it("不误收 Project stream connected shape", () => {
    const result = parseSessionNdjsonEnvelope({
      type: "connected",
      project_id: "project-1",
      last_event_id: 12,
    });

    expect(result.ok).toBe(false);
    if (!result.ok) {
      expect(result.error.message).toContain("ephemeral_epoch");
    }
  });

  it("未知 type 返回错误", () => {
    const result = parseSessionNdjsonEnvelope({
      type: "mystery",
      timestamp: 100,
    });

    expect(result.ok).toBe(false);
    if (!result.ok) {
      expect(result.error.message).toContain("未知 Session NDJSON 类型");
    }
  });
});
