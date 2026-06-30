import type { SessionNdjsonEnvelope } from "../../../generated/session-contracts";
import {
  parseGeneratedSessionNdjsonEnvelope,
  type GeneratedNdjsonEnvelopeParseResult,
  type GeneratedNdjsonEnvelopeValidationFailure,
  type GeneratedSessionNdjsonConnectedEnvelope,
  type GeneratedSessionNdjsonEphemeralEventEnvelope,
  type GeneratedSessionNdjsonEventEnvelope,
  type GeneratedSessionNdjsonHeartbeatEnvelope,
} from "../../../generated/ndjson-stream-validators";
import type { SessionEventEnvelope } from "./types";

export type SessionNdjsonConnectedEnvelope =
  GeneratedSessionNdjsonConnectedEnvelope;
export type SessionNdjsonEventEnvelope =
  GeneratedSessionNdjsonEventEnvelope;
export type SessionNdjsonEphemeralEventEnvelope =
  GeneratedSessionNdjsonEphemeralEventEnvelope;
export type SessionNdjsonHeartbeatEnvelope =
  GeneratedSessionNdjsonHeartbeatEnvelope;

export type SessionNdjsonEnvelopeParseResult =
  | Extract<GeneratedNdjsonEnvelopeParseResult<SessionNdjsonEnvelope>, { ok: true }>
  | { ok: false; error: Error };

function readOptionalString(value: unknown): string | null {
  if (typeof value !== "string") return null;
  const trimmed = value.trim();
  return trimmed.length > 0 ? trimmed : null;
}

function readOptionalNumber(value: unknown): number | undefined {
  if (value == null) return undefined;
  return typeof value === "number" && Number.isFinite(value) ? value : undefined;
}

function sessionNdjsonFailureToError(
  failure: GeneratedNdjsonEnvelopeValidationFailure,
): Error {
  switch (failure.reason) {
    case "not_object":
      return new Error("NDJSON 消息必须是对象");
    case "unknown_type":
      return new Error(`未知 Session NDJSON 类型: ${failure.actual_type}`);
    case "invalid_branch":
      return new Error(
        `Session stream ${failure.branch} ${failure.field_path} 必须是 ${failure.expected}`,
      );
  }
}

export function parseSessionNdjsonEnvelope(payload: unknown): SessionNdjsonEnvelopeParseResult {
  const result = parseGeneratedSessionNdjsonEnvelope(payload);
  if (result.ok) {
    return result;
  }
  return { ok: false, error: sessionNdjsonFailureToError(result.failure) };
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
