import type { StreamEvent } from '../types';
import { buildApiPath } from './origin';
import { FetchNdjsonStream, type NdjsonStreamLifecycle } from "./ndjsonStream";

export type ProjectEventStreamLifecycle = NdjsonStreamLifecycle;

export interface ProjectEventStreamConnection {
  close: () => void;
}

export interface ProjectEventStreamOptions {
  projectId: string;
  sinceId?: number;
  onEvent: (event: StreamEvent) => void;
  onLifecycleChange: (lifecycle: ProjectEventStreamLifecycle) => void;
  onError: (error: Error) => void;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return Boolean(value) && typeof value === "object";
}

function readEventCursor(event: StreamEvent): number | null {
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

function parseStreamEvent(value: unknown): StreamEvent | null {
  if (!isRecord(value) || typeof value.type !== "string" || !isRecord(value.data)) {
    return null;
  }

  switch (value.type) {
    case "Connected":
      return typeof value.data.last_event_id === "number"
        ? { type: "Connected", data: { last_event_id: value.data.last_event_id } }
        : null;
    case "StateChanged":
      return typeof value.data.id === "number" &&
        typeof value.data.project_id === "string" &&
        typeof value.data.entity_id === "string" &&
        typeof value.data.kind === "string" &&
        typeof value.data.created_at === "string"
        ? {
            type: "StateChanged",
            data: {
              id: value.data.id,
              project_id: value.data.project_id,
              entity_id: value.data.entity_id,
              kind: value.data.kind,
              payload: isRecord(value.data.payload) ? value.data.payload : {},
              backend_id: typeof value.data.backend_id === "string" ? value.data.backend_id : null,
              created_at: value.data.created_at,
            },
          }
        : null;
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
  return new FetchNdjsonStream<StreamEvent>({
    url: buildApiPath(`/events/stream/ndjson?${params.toString()}`),
    sinceId: options.sinceId,
    parsePayload: parseStreamEvent,
    readCursor: readEventCursor,
    onEvent: options.onEvent,
    onLifecycleChange: options.onLifecycleChange,
    onError: options.onError,
    connectionErrorMessage: "项目事件流连接失败",
    parseErrorMessage: "解析项目事件流消息失败",
  });
}
