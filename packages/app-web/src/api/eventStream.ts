import type {
  ProjectEventStreamEnvelope,
} from "../generated/project-contracts";
import { buildApiPath } from './origin';
import { FetchNdjsonStream, type NdjsonStreamLifecycle } from "./ndjsonStream";
import { parseProjectEventStreamEnvelopeResult } from "./projectEventStreamEnvelopeValidator";

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

export function readProjectEventStreamCursor(event: ProjectEventStreamEnvelope): number | null {
  switch (event.type) {
    case "Connected":
      return event.data.last_event_id;
    case "StateChanged":
      return event.data.id;
    case "ControlPlaneProjectionChanged":
    case "BackendRuntimeChanged":
    case "Heartbeat":
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
    parsePayload: (payload) => {
      const result = parseProjectEventStreamEnvelopeResult(payload);
      if (!result.ok) {
        options.onError(result.error);
        return null;
      }
      return result.envelope;
    },
    readCursor: readProjectEventStreamCursor,
    onEvent: options.onEvent,
    onLifecycleChange: options.onLifecycleChange,
    onError: options.onError,
    connectionErrorMessage: "项目事件流连接失败",
    parseErrorMessage: "解析项目事件流消息失败",
  });
}
