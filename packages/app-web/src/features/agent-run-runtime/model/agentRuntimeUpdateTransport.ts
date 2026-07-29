import { buildApiPath } from "../../../api/origin";
import { FetchNdjsonStream } from "../../../api/ndjsonStream";
import {
  decodeAgentRuntimeUpdate,
  type AgentRuntimeUpdate,
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
  onEvent: (update: AgentRuntimeUpdate) => void;
  onLifecycleChange: (lifecycle: AgentRuntimeConnectionLifecycle) => void;
  onError: (error: Error) => void;
}

export interface AgentRuntimeUpdateTransport {
  close: () => void;
}

export function parseAgentRuntimeUpdate(payload: unknown): AgentRuntimeUpdate | null {
  try {
    return decodeAgentRuntimeUpdate(payload);
  } catch {
    return null;
  }
}

export function createAgentRuntimeUpdateTransport(
  options: AgentRuntimeUpdateTransportOptions,
): AgentRuntimeUpdateTransport {
  return new FetchNdjsonStream<AgentRuntimeUpdate>({
    url: buildApiPath(
      agentRunScopedPath(options.agentRunTarget, "/runtime/updates"),
    ),
    parsePayload: parseAgentRuntimeUpdate,
    readCursor: () => null,
    onEvent: options.onEvent,
    onLifecycleChange: options.onLifecycleChange,
    onError: options.onError,
    connectionErrorMessage: "Agent Runtime update 连接失败",
    parseErrorMessage: "Agent Runtime update 解析失败",
  });
}
