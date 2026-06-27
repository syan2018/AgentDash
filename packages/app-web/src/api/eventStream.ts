import type { JsonValue } from "../generated/common-contracts";
import type {
  ProjectEventStreamEnvelope,
  ProjectStateChangeKind,
} from "../generated/project-contracts";
import { buildApiPath } from './origin';
import { FetchNdjsonStream, type NdjsonStreamLifecycle } from "./ndjsonStream";

export type ProjectEventStreamLifecycle = NdjsonStreamLifecycle;
export type {
  ProjectEventStreamEnvelope,
  ProjectStateChange,
  ProjectStateChangeKind,
} from "../generated/project-contracts";

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

const PROJECT_STATE_CHANGE_KINDS = new Set<string>([
  "story_created",
  "story_updated",
  "story_status_changed",
  "story_deleted",
  "task_created",
  "task_updated",
  "task_status_changed",
  "task_deleted",
]);

function isProjectStateChangeKind(value: unknown): value is ProjectStateChangeKind {
  return typeof value === "string" && PROJECT_STATE_CHANGE_KINDS.has(value);
}

function isProjectEventStreamEnvelope(value: unknown): value is ProjectEventStreamEnvelope {
  if (!isRecord(value) || typeof value.type !== "string" || !isRecord(value.data)) {
    return false;
  }

  switch (value.type) {
    case "Connected":
      return typeof value.data.last_event_id === "number";
    case "StateChanged":
      return typeof value.data.id === "number" &&
        typeof value.data.project_id === "string" &&
        typeof value.data.entity_id === "string" &&
        isProjectStateChangeKind(value.data.kind) &&
        isJsonObject(value.data.payload) &&
        (value.data.backend_id === null || typeof value.data.backend_id === "string") &&
        typeof value.data.created_at === "string";
    case "BackendRuntimeChanged":
      return typeof value.data.backend_id === "string";
    case "Heartbeat":
      return typeof value.data.timestamp === "number";
    default:
      return false;
  }
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
  return isProjectEventStreamEnvelope(value) ? value : null;
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
