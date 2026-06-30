import type { BackboneEnvelope } from "../../../generated/backbone-protocol";
import type { SessionNdjsonEnvelope } from "../../../generated/session-contracts";
import {
  isFiniteNumber,
  isNonEmptyString,
  isOptionalFiniteNumber,
  isOptionalString,
  isRecord,
  type NdjsonEnvelopeParseResult,
} from "../../../api/ndjsonEnvelopeValidator";
import type { SessionEventEnvelope } from "./types";

export type SessionNdjsonConnectedEnvelope = Extract<
  SessionNdjsonEnvelope,
  { type: "connected" }
>;
export type SessionNdjsonEventEnvelope = Extract<
  SessionNdjsonEnvelope,
  { type: "event" }
>;
export type SessionNdjsonEphemeralEventEnvelope = Extract<
  SessionNdjsonEnvelope,
  { type: "ephemeral_event" }
>;
export type SessionNdjsonHeartbeatEnvelope = Extract<
  SessionNdjsonEnvelope,
  { type: "heartbeat" }
>;

export type SessionNdjsonEnvelopeParseResult =
  NdjsonEnvelopeParseResult<SessionNdjsonEnvelope>;

function isBackboneEnvelope(value: unknown): value is BackboneEnvelope {
  if (!isRecord(value)) return false;
  return typeof value.event === "object" && value.event !== null &&
    typeof value.sessionId === "string";
}

function readOptionalString(value: unknown): string | null {
  if (typeof value !== "string") return null;
  const trimmed = value.trim();
  return trimmed.length > 0 ? trimmed : null;
}

function readOptionalNumber(value: unknown): number | undefined {
  if (value == null) return undefined;
  return isFiniteNumber(value) ? value : undefined;
}

function isConnectedEnvelope(value: unknown): value is SessionNdjsonConnectedEnvelope {
  return isRecord(value) &&
    value.type === "connected" &&
    isFiniteNumber(value.last_event_id) &&
    isFiniteNumber(value.ephemeral_epoch);
}

function isEventEnvelope(value: unknown): value is SessionNdjsonEventEnvelope {
  return isSessionEventEnvelope(value, "event");
}

function isEphemeralEventEnvelope(value: unknown): value is SessionNdjsonEphemeralEventEnvelope {
  return isSessionEventEnvelope(value, "ephemeral_event");
}

function isSessionEventEnvelope(
  value: unknown,
  type: "event" | "ephemeral_event",
): value is SessionNdjsonEventEnvelope | SessionNdjsonEphemeralEventEnvelope {
  return isRecord(value) &&
    value.type === type &&
    isNonEmptyString(value.session_id) &&
    isFiniteNumber(value.event_seq) &&
    isFiniteNumber(value.occurred_at_ms) &&
    isFiniteNumber(value.committed_at_ms) &&
    isNonEmptyString(value.session_update_type) &&
    isOptionalString(value.turn_id) &&
    isOptionalFiniteNumber(value.entry_index) &&
    isOptionalString(value.tool_call_id) &&
    isBackboneEnvelope(value.notification);
}

function readSessionEventEnvelopeError(
  value: Record<string, unknown>,
): Error | null {
  if (!isFiniteNumber(value.event_seq)) {
    return new Error("Session stream event 缺少合法 event_seq");
  }
  if (!isNonEmptyString(value.session_id)) {
    return new Error("Session stream event 缺少 session_id");
  }
  if (!isFiniteNumber(value.occurred_at_ms)) {
    return new Error("Session stream event 缺少 occurred_at_ms");
  }
  if (!isFiniteNumber(value.committed_at_ms)) {
    return new Error("Session stream event 缺少 committed_at_ms");
  }
  if (!isNonEmptyString(value.session_update_type)) {
    return new Error("Session stream event 缺少 session_update_type");
  }
  if (!isBackboneEnvelope(value.notification)) {
    return new Error("Session stream event 缺少合法 notification");
  }
  if (!isOptionalString(value.turn_id)) {
    return new Error("Session stream event turn_id 必须是字符串");
  }
  if (!isOptionalFiniteNumber(value.entry_index)) {
    return new Error("Session stream event entry_index 必须是数字");
  }
  if (!isOptionalString(value.tool_call_id)) {
    return new Error("Session stream event tool_call_id 必须是字符串");
  }
  return null;
}

function isHeartbeatEnvelope(value: unknown): value is SessionNdjsonHeartbeatEnvelope {
  return isRecord(value) &&
    value.type === "heartbeat" &&
    isFiniteNumber(value.timestamp);
}

export function parseSessionNdjsonEnvelope(payload: unknown): SessionNdjsonEnvelopeParseResult {
  if (!isRecord(payload)) {
    return { ok: false, error: new Error("NDJSON 消息必须是对象") };
  }

  switch (payload.type) {
    case "connected":
      return isConnectedEnvelope(payload)
        ? { ok: true, kind: "connected", envelope: payload }
        : { ok: false, error: new Error("Session stream connected 缺少合法 last_event_id 或 ephemeral_epoch") };
    case "event": {
      const error = readSessionEventEnvelopeError(payload);
      if (error) return { ok: false, error };
      return isEventEnvelope(payload)
        ? { ok: true, kind: "event", envelope: payload }
        : { ok: false, error: new Error("Session stream event shape 不合法") };
    }
    case "ephemeral_event": {
      const error = readSessionEventEnvelopeError(payload);
      if (error) return { ok: false, error };
      return isEphemeralEventEnvelope(payload)
        ? { ok: true, kind: "ephemeral_event", envelope: payload }
        : { ok: false, error: new Error("Session stream ephemeral_event shape 不合法") };
    }
    case "heartbeat":
      return isHeartbeatEnvelope(payload)
        ? { ok: true, kind: "heartbeat", envelope: payload }
        : { ok: false, error: new Error("Session stream heartbeat 缺少合法 timestamp") };
    default:
      return { ok: false, error: new Error(`未知 Session NDJSON 类型: ${String(payload.type)}`) };
  }
}

export function toSessionEventEnvelope(
  payload: SessionNdjsonEventEnvelope | SessionNdjsonEphemeralEventEnvelope,
): SessionEventEnvelope {
  return {
    session_id: payload.session_id,
    event_seq: payload.event_seq,
    occurred_at_ms: payload.occurred_at_ms,
    committed_at_ms: payload.committed_at_ms,
    session_update_type: payload.session_update_type,
    turn_id: readOptionalString(payload.turn_id) ?? undefined,
    entry_index: readOptionalNumber(payload.entry_index),
    tool_call_id: readOptionalString(payload.tool_call_id) ?? undefined,
    notification: payload.notification,
    ephemeral: payload.type === "ephemeral_event",
  };
}

export function parseSessionEventEnvelopePayload(
  payload: unknown,
): { event: SessionEventEnvelope | null; error: Error | null } {
  const result = parseSessionNdjsonEnvelope(payload);
  if (!result.ok) {
    return { event: null, error: result.error };
  }
  if (result.kind !== "event" && result.kind !== "ephemeral_event") {
    return { event: null, error: new Error("Session stream payload 不是 event 分支") };
  }
  return { event: toSessionEventEnvelope(result.envelope), error: null };
}
