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

export function parseLiveEvent(payload: unknown): AgentLiveEvent | null {
  if (!payload || typeof payload !== "object") return null;
  const event = payload as Record<string, unknown>;
  const record = event.record;
  if (!record || typeof record !== "object") return null;
  const canonical = record as Record<string, unknown>;
  const presentation = canonical.presentation;
  if (!presentation || typeof presentation !== "object") return null;
  const envelope = (presentation as Record<string, unknown>).envelope;
  if (!envelope || typeof envelope !== "object") return null;
  const backboneEvent = (envelope as Record<string, unknown>).event;
  if (
    typeof event.source !== "string" ||
    typeof event.sequence !== "string" ||
    !/^(0|[1-9]\d*)$/.test(event.sequence) ||
    typeof canonical.presentation_id !== "string" ||
    canonical.presentation_id.length === 0 ||
    !backboneEvent ||
    typeof backboneEvent !== "object" ||
    typeof (backboneEvent as Record<string, unknown>).type !== "string"
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
