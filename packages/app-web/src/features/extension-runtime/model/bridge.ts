import type { JsonValue } from "../../../generated/common-contracts";

export const EXTENSION_BRIDGE_CHANNEL = "agentdash.extension";

export interface ExtensionBridgeRequestMessage {
  channel: typeof EXTENSION_BRIDGE_CHANNEL;
  kind: "request";
  request_id: string;
  method: string;
  params: Record<string, unknown>;
}

export interface ExtensionBridgeEventMessage {
  channel: typeof EXTENSION_BRIDGE_CHANNEL;
  kind: "event";
  type: string;
  payload: JsonValue;
}

export type ExtensionBridgeMessage = ExtensionBridgeRequestMessage | ExtensionBridgeEventMessage;

export function parseExtensionBridgeMessage(raw: unknown): ExtensionBridgeMessage | null {
  const value = asRecord(raw);
  if (!value || value.channel !== EXTENSION_BRIDGE_CHANNEL) return null;
  if (value.kind === "request") {
    const requestId = typeof value.request_id === "string" ? value.request_id.trim() : "";
    const method = typeof value.method === "string" ? value.method.trim() : "";
    if (!requestId || !method) return null;
    return {
      channel: EXTENSION_BRIDGE_CHANNEL,
      kind: "request",
      request_id: requestId,
      method,
      params: asRecord(value.params) ?? {},
    };
  }
  if (value.kind === "event") {
    const type = typeof value.type === "string" ? value.type.trim() : "";
    if (!type) return null;
    return {
      channel: EXTENSION_BRIDGE_CHANNEL,
      kind: "event",
      type,
      payload: toJsonValue(value.payload),
    };
  }
  return null;
}

export function bridgeParamString(
  params: Record<string, unknown>,
  key: string,
): string {
  const value = params[key];
  return typeof value === "string" ? value.trim() : "";
}

export function toJsonValue(raw: unknown): JsonValue {
  if (raw === null || typeof raw === "string" || typeof raw === "boolean") return raw;
  if (typeof raw === "number") return Number.isFinite(raw) ? raw : null;
  if (Array.isArray(raw)) return raw.map(toJsonValue);
  const record = asRecord(raw);
  if (!record) return null;
  const result: { [key: string]: JsonValue } = {};
  for (const [key, value] of Object.entries(record)) {
    result[key] = toJsonValue(value);
  }
  return result;
}

function asRecord(raw: unknown): Record<string, unknown> | null {
  return raw != null && typeof raw === "object" && !Array.isArray(raw)
    ? raw as Record<string, unknown>
    : null;
}
