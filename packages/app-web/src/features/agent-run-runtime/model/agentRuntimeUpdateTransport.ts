import { buildApiPath } from "../../../api/origin";
import { FetchNdjsonStream } from "../../../api/ndjsonStream";
import {
  decodeAgentRuntimeStreamFrame,
  type AgentRuntimeStreamFrame,
} from "../../../generated/agent-runtime-codecs";
import {
  agentRunScopedPath,
  type AgentRunRuntimeTarget,
} from "../../../services/agentRunRuntime";

export type AgentRuntimeConnectionLifecycle =
  | "connecting"
  | "connected"
  | "reconnecting"
  | "closed";

export interface AgentRuntimeUpdateTransportOptions {
  agentRunTarget: AgentRunRuntimeTarget;
  onEvent: (frame: AgentRuntimeStreamFrame) => void;
  onLifecycleChange: (lifecycle: AgentRuntimeConnectionLifecycle) => void;
  onError: (error: Error) => void;
}

export interface AgentRuntimeUpdateTransport {
  close: () => void;
}

const RESET_REASONS = new Set([
  "lagged",
  "sequence_gap",
  "source_mismatch",
  "protocol_error",
  "binding_replaced",
  "transport_disconnected",
]);

export function parseAgentRuntimeStreamFrame(
  payload: unknown,
): AgentRuntimeStreamFrame | null {
  try {
    if (
      typeof payload !== "object"
      || payload == null
      || !("kind" in payload)
    ) {
      return null;
    }
    const frame = decodeAgentRuntimeStreamFrame(payload);
    if (frame.kind === "baseline") {
      return typeof frame.connection_epoch === "bigint"
          && typeof frame.view === "object"
          && frame.view != null
        ? frame
        : null;
    }
    if (frame.kind === "update") {
      return typeof frame.connection_epoch === "bigint"
          && typeof frame.lane_sequence === "bigint"
          && (frame.state == null || typeof frame.state === "object")
          && Array.isArray(frame.presentations)
        ? frame
        : null;
    }
    if (frame.kind === "reset_required") {
      return typeof frame.connection_epoch === "bigint"
          && RESET_REASONS.has(frame.reason)
          && (frame.last_sequence == null
            || typeof frame.last_sequence === "bigint")
        ? frame
        : null;
    }
    return null;
  } catch {
    return null;
  }
}

export function createAgentRuntimeUpdateTransport(
  options: AgentRuntimeUpdateTransportOptions,
): AgentRuntimeUpdateTransport {
  return new FetchNdjsonStream<AgentRuntimeStreamFrame>({
    url: buildApiPath(
      agentRunScopedPath(options.agentRunTarget, "/runtime/updates"),
    ),
    parsePayload: parseAgentRuntimeStreamFrame,
    readCursor: () => null,
    onEvent: options.onEvent,
    onLifecycleChange: options.onLifecycleChange,
    onError: options.onError,
    connectionErrorMessage: "Agent Runtime update 连接失败",
    parseErrorMessage: "Agent Runtime update 解析失败",
  });
}
