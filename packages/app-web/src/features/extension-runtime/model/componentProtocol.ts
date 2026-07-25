const COMPONENT_CHANNEL = "agentdash.component.v1";
const MAX_COMPONENT_PAYLOAD_BYTES = 64 * 1024;
const MAX_COMPONENT_EVENTS_PER_SECOND = 30;

export type ComponentJsonSchema = boolean | Record<string, unknown>;

export interface ExtensionUiComponentDescriptor {
  component_key: string;
  contract_version: 1;
  renderer: { kind: "iframe"; entry: string };
  props_schema: ComponentJsonSchema;
  events_schema: Record<string, ComponentJsonSchema>;
  state_projection_schema: ComponentJsonSchema;
  slots: string[];
  sizing: { min_width: number; min_height: number; max_width?: number; max_height?: number };
  sandbox_profile: "isolated_v1";
}

export type ComponentInboundMessage =
  | { channel: typeof COMPONENT_CHANNEL; kind: "ready" }
  | { channel: typeof COMPONENT_CHANNEL; kind: "resize"; width: number; height: number }
  | { channel: typeof COMPONENT_CHANNEL; kind: "event"; request_id: string; event_type: string; payload: unknown }
  | { channel: typeof COMPONENT_CHANNEL; kind: "diagnostic"; level: "debug" | "info" | "warn" | "error"; message: string };

export function parseComponentMessage(value: unknown): ComponentInboundMessage | null {
  const record = asRecord(value);
  if (!record || record.channel !== COMPONENT_CHANNEL || typeof record.kind !== "string") return null;
  if (record.kind === "ready") return { channel: COMPONENT_CHANNEL, kind: "ready" };
  if (record.kind === "resize" && finiteNumber(record.width) && finiteNumber(record.height)) {
    return { channel: COMPONENT_CHANNEL, kind: "resize", width: record.width, height: record.height };
  }
  if (record.kind === "event" && nonEmptyString(record.request_id) && nonEmptyString(record.event_type)
    && payloadBytes(record.payload) <= MAX_COMPONENT_PAYLOAD_BYTES) {
    return { channel: COMPONENT_CHANNEL, kind: "event", request_id: record.request_id, event_type: record.event_type, payload: record.payload };
  }
  if (record.kind === "diagnostic" && ["debug", "info", "warn", "error"].includes(String(record.level))
    && nonEmptyString(record.message) && record.message.length <= 2_000) {
    return { channel: COMPONENT_CHANNEL, kind: "diagnostic", level: record.level as "debug" | "info" | "warn" | "error", message: record.message };
  }
  return null;
}

export function validateComponentPayload(schema: ComponentJsonSchema, value: unknown): boolean {
  if (schema === true) return true;
  if (schema === false) return false;
  if (Array.isArray(schema.enum) && !schema.enum.some((item) => deepEqual(item, value))) return false;
  const type = typeof schema.type === "string" ? schema.type : null;
  if (type && !matchesType(type, value)) return false;
  if (type === "object" || (type === null && asRecord(schema.properties))) {
    const record = asRecord(value);
    if (!record) return false;
    const required = Array.isArray(schema.required)
      ? schema.required.filter((item): item is string => typeof item === "string")
      : [];
    if (required.some((key) => !(key in record))) return false;
    const properties = asRecord(schema.properties) ?? {};
    for (const [key, item] of Object.entries(record)) {
      const child = properties[key];
      if (child !== undefined) {
        if (!isSchema(child) || !validateComponentPayload(child, item)) return false;
      } else if (schema.additionalProperties === false) return false;
    }
  }
  if (type === "array") {
    if (!Array.isArray(value)) return false;
    if (typeof schema.maxItems === "number" && value.length > schema.maxItems) return false;
    if (isSchema(schema.items) && value.some((item) => !validateComponentPayload(schema.items as ComponentJsonSchema, item))) return false;
  }
  if (typeof value === "string") {
    if (typeof schema.maxLength === "number" && value.length > schema.maxLength) return false;
    if (typeof schema.minLength === "number" && value.length < schema.minLength) return false;
  }
  return true;
}

export function clampComponentSize(descriptor: ExtensionUiComponentDescriptor, width: number, height: number) {
  return {
    width: clamp(width, descriptor.sizing.min_width, descriptor.sizing.max_width),
    height: clamp(height, descriptor.sizing.min_height, descriptor.sizing.max_height),
  };
}

export class ComponentEventRateGate {
  private timestamps: number[] = [];

  admit(now: number): boolean {
    this.timestamps = this.timestamps.filter((timestamp) => now - timestamp < 1_000);
    if (this.timestamps.length >= MAX_COMPONENT_EVENTS_PER_SECOND) return false;
    this.timestamps.push(now);
    return true;
  }
}

export function componentConnectMessage() {
  return { channel: COMPONENT_CHANNEL, kind: "connect", contract_version: 1 } as const;
}

export function componentHostMessage(kind: string, payload: Record<string, unknown>) {
  return { channel: COMPONENT_CHANNEL, kind, ...payload };
}

function matchesType(type: string, value: unknown): boolean {
  switch (type) {
    case "object": return asRecord(value) !== null;
    case "array": return Array.isArray(value);
    case "string": return typeof value === "string";
    case "number": return typeof value === "number" && Number.isFinite(value);
    case "integer": return typeof value === "number" && Number.isInteger(value);
    case "boolean": return typeof value === "boolean";
    case "null": return value === null;
    default: return false;
  }
}

function clamp(value: number, minimum: number, maximum?: number): number {
  const finite = Number.isFinite(value) ? value : minimum;
  return Math.min(Math.max(finite, minimum), maximum ?? Number.MAX_SAFE_INTEGER);
}

function payloadBytes(value: unknown): number {
  try { return new TextEncoder().encode(JSON.stringify(value)).byteLength; }
  catch { return Number.MAX_SAFE_INTEGER; }
}

function deepEqual(left: unknown, right: unknown): boolean {
  try { return JSON.stringify(left) === JSON.stringify(right); }
  catch { return false; }
}

function asRecord(value: unknown): Record<string, unknown> | null {
  return value !== null && typeof value === "object" && !Array.isArray(value)
    ? value as Record<string, unknown>
    : null;
}

function nonEmptyString(value: unknown): value is string {
  return typeof value === "string" && value.trim() !== "";
}

function finiteNumber(value: unknown): value is number {
  return typeof value === "number" && Number.isFinite(value);
}

function isSchema(value: unknown): value is ComponentJsonSchema {
  return typeof value === "boolean" || asRecord(value) !== null;
}
