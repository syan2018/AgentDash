import type { JsonValue } from "../generated/common-contracts";

export type NdjsonEnvelopeParseResult<TEnvelope extends { type: string }> =
  | {
    [TKind in TEnvelope["type"]]: {
      ok: true;
      kind: TKind;
      envelope: Extract<TEnvelope, { type: TKind }>;
    };
  }[TEnvelope["type"]]
  | { ok: false; error: Error };

export function isRecord(value: unknown): value is Record<string, unknown> {
  return Boolean(value) && typeof value === "object" && !Array.isArray(value);
}

export function isFiniteNumber(value: unknown): value is number {
  return typeof value === "number" && Number.isFinite(value);
}

export function isNonEmptyString(value: unknown): value is string {
  return typeof value === "string" && value.trim().length > 0;
}

export function isOptionalString(value: unknown): value is string | undefined {
  return value == null || typeof value === "string";
}

export function isOptionalFiniteNumber(value: unknown): value is number | undefined {
  return value == null || isFiniteNumber(value);
}

export function isJsonValue(value: unknown): value is JsonValue {
  if (value === null) return true;
  if (typeof value === "string" || typeof value === "boolean") return true;
  if (typeof value === "number") return Number.isFinite(value);
  if (Array.isArray(value)) return value.every(isJsonValue);
  if (isRecord(value)) return Object.values(value).every(isJsonValue);
  return false;
}

export function isJsonObject(value: unknown): value is Record<string, JsonValue> {
  return isRecord(value) && Object.values(value).every(isJsonValue);
}
