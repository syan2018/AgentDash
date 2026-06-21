import type { JsonValue } from "../generated/common-contracts";
import { buildApiPath } from './origin';
import { FetchNdjsonStream, type NdjsonStreamLifecycle } from "./ndjsonStream";

export type ProjectEventStreamLifecycle = NdjsonStreamLifecycle;

export interface ProjectStateChange {
  id: number;
  project_id: string;
  entity_id: string;
  kind: string;
  payload: Record<string, JsonValue>;
  backend_id: string | null;
  created_at: string;
}

export type ProjectEventStreamEnvelope =
  | { type: "Connected"; data: { last_event_id: number } }
  | { type: "StateChanged"; data: ProjectStateChange }
  | { type: "BackendRuntimeChanged"; data: { backend_id: string } }
  | { type: "Heartbeat"; data: { timestamp: number } };

export interface ProjectEventStreamConnection {
  close: () => void;
}

export interface ProjectEventStreamOptions {
  projectId: string;
  sinceId?: number;
  onEvent: (event: ProjectEventStreamEnvelope) => void;
  onLifecycleChange: (lifecycle: ProjectEventStreamLifecycle) => void;
  onError: (error: Error) => void;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return Boolean(value) && typeof value === "object" && !Array.isArray(value);
}

function isJsonValue(value: unknown): value is JsonValue {
  if (value === null) return true;
  if (typeof value === "string" || typeof value === "boolean") {
    return true;
  }
  if (typeof value === "number") return Number.isFinite(value);
  if (Array.isArray(value)) {
    return value.every(isJsonValue);
  }
  if (isRecord(value)) {
    return Object.values(value).every(isJsonValue);
  }
  return false;
}

function isJsonObject(value: unknown): value is Record<string, JsonValue> {
  return isRecord(value) && Object.values(value).every(isJsonValue);
}

export function readProjectEventStreamCursor(event: ProjectEventStreamEnvelope): number | null {
  switch (event.type) {
    case "Connected":
      return event.data.last_event_id;
    case "StateChanged":
      return event.data.id;
    case "BackendRuntimeChanged":
    case "Heartbeat":
      return null;
  }
}

export function parseProjectEventStreamEnvelope(value: unknown): ProjectEventStreamEnvelope | null {
  if (!isRecord(value) || typeof value.type !== "string" || !isRecord(value.data)) {
    return null;
  }

  switch (value.type) {
    case "Connected":
      return typeof value.data.last_event_id === "number"
        ? { type: "Connected", data: { last_event_id: value.data.last_event_id } }
        : null;
    case "StateChanged": {
      const payload = isJsonObject(value.data.payload) ? value.data.payload : null;
      return typeof value.data.id === "number" &&
        typeof value.data.project_id === "string" &&
        typeof value.data.entity_id === "string" &&
        typeof value.data.kind === "string" &&
        typeof value.data.created_at === "string" &&
        payload !== null
        ? {
            type: "StateChanged",
            data: {
              id: value.data.id,
              project_id: value.data.project_id,
              entity_id: value.data.entity_id,
              kind: value.data.kind,
              payload,
              backend_id: typeof value.data.backend_id === "string" ? value.data.backend_id : null,
              created_at: value.data.created_at,
            },
          }
        : null;
    }
    case "BackendRuntimeChanged":
      return typeof value.data.backend_id === "string"
        ? { type: "BackendRuntimeChanged", data: { backend_id: value.data.backend_id } }
        : null;
    case "Heartbeat":
      return typeof value.data.timestamp === "number"
        ? { type: "Heartbeat", data: { timestamp: value.data.timestamp } }
        : null;
    default:
      return null;
  }
}

export function connectProjectEventStream(
  options: ProjectEventStreamOptions,
): ProjectEventStreamConnection {
  const params = new URLSearchParams({ project_id: options.projectId });
  return new FetchNdjsonStream<ProjectEventStreamEnvelope>({
    url: buildApiPath(`/events/stream/ndjson?${params.toString()}`),
    sinceId: options.sinceId,
    parsePayload: parseProjectEventStreamEnvelope,
    readCursor: readProjectEventStreamCursor,
    onEvent: options.onEvent,
    onLifecycleChange: options.onLifecycleChange,
    onError: options.onError,
    connectionErrorMessage: "项目事件流连接失败",
    parseErrorMessage: "解析项目事件流消息失败",
  });
}
