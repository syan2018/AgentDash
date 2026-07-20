import type { AgentLiveEvent } from "../../../generated/agent-service-api";
import { buildApiPath } from "../../../api/origin";
import { FetchNdjsonStream } from "../../../api/ndjsonStream";
import {
  agentRunScopedPath,
  type AgentRunRuntimeTarget,
} from "../../../services/agentRunRuntime";

export type ManagedRuntimeFeedLifecycle =
  | "connecting"
  | "connected"
  | "reconnecting"
  | "closed";

export interface ManagedRuntimeFeedTransportOptions {
  agentRunTarget: AgentRunRuntimeTarget;
  onEvent: (event: AgentLiveEvent) => void;
  onLifecycleChange: (lifecycle: ManagedRuntimeFeedLifecycle) => void;
  onError: (error: Error) => void;
}

export interface ManagedRuntimeFeedTransport {
  close: () => void;
}

function parseLiveEvent(payload: unknown): AgentLiveEvent | null {
  if (!payload || typeof payload !== "object") return null;
  const event = payload as Record<string, unknown>;
  const body = event.payload;
  if (
    typeof event.source !== "string" ||
    typeof event.turn_id !== "string" ||
    typeof event.item_id !== "string" ||
    typeof event.sequence !== "string" ||
    !body ||
    typeof body !== "object" ||
    typeof (body as Record<string, unknown>).kind !== "string"
  ) {
    return null;
  }
  return payload as AgentLiveEvent;
}

export function createManagedRuntimeFeedTransport(
  options: ManagedRuntimeFeedTransportOptions,
): ManagedRuntimeFeedTransport {
  return new FetchNdjsonStream<AgentLiveEvent>({
    url: buildApiPath(
      agentRunScopedPath(options.agentRunTarget, "/runtime/live"),
    ),
    parsePayload: parseLiveEvent,
    readCursor: () => null,
    onEvent: options.onEvent,
    onLifecycleChange: options.onLifecycleChange,
    onError: options.onError,
    connectionErrorMessage: "Agent live stream 连接失败",
    parseErrorMessage: "Agent live stream 消息解析失败",
  });
}
