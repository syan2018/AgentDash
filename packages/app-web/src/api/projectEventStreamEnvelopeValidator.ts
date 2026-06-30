import type {
  ProjectEventStreamEnvelope,
} from "../generated/project-contracts";
import {
  isFiniteNumber,
  isJsonObject,
  isNonEmptyString,
  isRecord,
  type NdjsonEnvelopeParseResult,
} from "./ndjsonEnvelopeValidator";

export type ProjectEventStreamConnectedEnvelope = Extract<
  ProjectEventStreamEnvelope,
  { type: "Connected" }
>;
export type ProjectEventStreamStateChangedEnvelope = Extract<
  ProjectEventStreamEnvelope,
  { type: "StateChanged" }
>;
export type ProjectEventStreamBackendRuntimeChangedEnvelope = Extract<
  ProjectEventStreamEnvelope,
  { type: "BackendRuntimeChanged" }
>;
export type ProjectEventStreamHeartbeatEnvelope = Extract<
  ProjectEventStreamEnvelope,
  { type: "Heartbeat" }
>;

export type ProjectEventStreamEnvelopeParseResult =
  NdjsonEnvelopeParseResult<ProjectEventStreamEnvelope>;

function isConnectedEnvelope(value: unknown): value is ProjectEventStreamConnectedEnvelope {
  return isRecord(value) &&
    value.type === "Connected" &&
    isRecord(value.data) &&
    isFiniteNumber(value.data.last_event_id);
}

function isStateChangedEnvelope(value: unknown): value is ProjectEventStreamStateChangedEnvelope {
  return isRecord(value) &&
    value.type === "StateChanged" &&
    isRecord(value.data) &&
    isFiniteNumber(value.data.id) &&
    isNonEmptyString(value.data.project_id) &&
    isNonEmptyString(value.data.entity_id) &&
    isNonEmptyString(value.data.kind) &&
    isJsonObject(value.data.payload) &&
    (value.data.backend_id === null || typeof value.data.backend_id === "string") &&
    isNonEmptyString(value.data.created_at);
}

function isBackendRuntimeChangedEnvelope(
  value: unknown,
): value is ProjectEventStreamBackendRuntimeChangedEnvelope {
  return isRecord(value) &&
    value.type === "BackendRuntimeChanged" &&
    isRecord(value.data) &&
    isNonEmptyString(value.data.backend_id);
}

function isHeartbeatEnvelope(value: unknown): value is ProjectEventStreamHeartbeatEnvelope {
  return isRecord(value) &&
    value.type === "Heartbeat" &&
    isRecord(value.data) &&
    isFiniteNumber(value.data.timestamp);
}

export function parseProjectEventStreamEnvelopeResult(
  value: unknown,
): ProjectEventStreamEnvelopeParseResult {
  if (!isRecord(value)) {
    return { ok: false, error: new Error("Project event stream envelope 必须是对象") };
  }

  switch (value.type) {
    case "Connected":
      return isConnectedEnvelope(value)
        ? { ok: true, kind: "Connected", envelope: value }
        : { ok: false, error: new Error("Project event stream Connected 缺少合法 data.last_event_id") };
    case "StateChanged":
      return isStateChangedEnvelope(value)
        ? { ok: true, kind: "StateChanged", envelope: value }
        : { ok: false, error: new Error("Project event stream StateChanged shape 不合法") };
    case "BackendRuntimeChanged":
      return isBackendRuntimeChangedEnvelope(value)
        ? { ok: true, kind: "BackendRuntimeChanged", envelope: value }
        : { ok: false, error: new Error("Project event stream BackendRuntimeChanged 缺少合法 data.backend_id") };
    case "Heartbeat":
      return isHeartbeatEnvelope(value)
        ? { ok: true, kind: "Heartbeat", envelope: value }
        : { ok: false, error: new Error("Project event stream Heartbeat 缺少合法 data.timestamp") };
    default:
      return { ok: false, error: new Error(`未知 Project event stream 类型: ${String(value.type)}`) };
  }
}

export function parseProjectEventStreamEnvelope(value: unknown): ProjectEventStreamEnvelope | null {
  const result = parseProjectEventStreamEnvelopeResult(value);
  return result.ok ? result.envelope : null;
}
