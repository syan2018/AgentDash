import type { BackboneEvent } from "../../../generated/backbone-protocol";
import type { ConversationEffectiveExecutorConfigView } from "../../../generated/project-agent-contracts";
import type { ExecutorConfigSource } from "../../executor-selector/model/types";
import type { TaskSessionExecutorSummary } from "../../../types/context";
import type { ProjectAgentExecutor } from "../../../types";
import type { SessionEventEnvelope } from "../model/types";
import type { AgentRunRuntimeTarget } from "../../../services/agentRunRuntime";
import type { AgentRuntimeView } from "../../../generated/agent-runtime-validators";
import { extractPlatformEventType } from "../model/platformEvent";
import { shouldNotifyRenderableSystemEvent } from "../model/systemEventPolicy";
import type {
  SessionChatCommandModel,
  SessionChatCommandState,
  SessionChatInitialSubmit,
  SessionChatSubmitIntent,
} from "./SessionChatViewTypes";

function runtimeCommandAvailable(
  view: AgentRuntimeView,
  command: "submit_input" | "steer" | "interrupt" | "request_compaction",
): boolean {
  return view.observation.command_availability[command]?.status === "available";
}

function runtimeCommandReason(
  view: AgentRuntimeView,
  command: "submit_input" | "steer" | "interrupt" | "request_compaction",
): string | undefined {
  const availability = view.observation.command_availability[command];
  return availability?.status === "unavailable"
    ? availability.reason
    : undefined;
}

export function applyAgentRuntimeControlToChatCommandState(
  productState: SessionChatCommandState,
  view: AgentRuntimeView | null,
): SessionChatCommandState {
  if (!view || productState.mode !== "runtime") return productState;
  const active = view.observation.execution.active_turn != null;
  const submitAvailabilityKey = runtimeCommandAvailable(view, "submit_input")
    || !runtimeCommandAvailable(view, "steer")
    ? "submit_input"
    : "steer";
  const availabilityKey = (
    command: SessionChatCommandModel,
  ): "submit_input" | "steer" | "interrupt" | "request_compaction" | null => {
    if (command.runtimeCommand === "interrupt") return "interrupt";
    if (command.runtimeCommand === "request_compaction") return "request_compaction";
    if (command.runtimeCommand === "submit_input") {
      return submitAvailabilityKey;
    }
    if (command.kind === "submit_message") return submitAvailabilityKey;
    if (command.kind === "cancel") return "interrupt";
    if (command.kind === "compact_context") return "request_compaction";
    return null;
  };
  const projectCommand = (
    command: SessionChatCommandModel,
  ): SessionChatCommandModel => {
    const key = availabilityKey(command);
    if (!key) return command;
    const enabled = runtimeCommandAvailable(view, key);
    return {
      ...command,
      enabled,
      unavailable_reason: enabled ? undefined : runtimeCommandReason(view, key),
      disabled_code: enabled ? undefined : "runtime_command_unavailable",
    };
  };
  const commands = productState.commands.map(projectCommand);
  let cancelCommand = productState.cancelCommand
    ? projectCommand(productState.cancelCommand)
    : undefined;
  if (!cancelCommand) {
    const enabled = runtimeCommandAvailable(view, "interrupt");
    cancelCommand = {
      command_id: "runtime:interrupt",
      kind: "cancel",
      enabled,
      unavailable_reason: enabled
        ? undefined
        : runtimeCommandReason(view, "interrupt"),
      disabled_code: enabled ? undefined : "runtime_command_unavailable",
      requires_input: false,
      executor_config_policy: "forbidden",
    };
    commands.push(cancelCommand);
  }
  return {
    ...productState,
    executionStatus: active ? "running_active" : "ready",
    activeTurnId: view.observation.execution.active_turn?.turn_id ?? null,
    commands,
    cancelCommand,
  };
}

export function resolveSessionInitialSubmit(input: {
  initialSubmit?: SessionChatInitialSubmit;
  isConnected: boolean;
  historyReplayBoundarySeq: number | null;
  isSending: boolean;
  commands: SessionChatCommandModel[];
  primaryCommandId?: string;
}): SessionChatSubmitIntent | null {
  if (
    !input.initialSubmit
    || !input.isConnected
    || input.historyReplayBoundarySeq == null
    || input.isSending
  ) {
    return null;
  }
  const command = input.commands.find(
    (candidate) => candidate.command_id === input.primaryCommandId,
  );
  if (!command?.enabled) return null;
  return {
    ...input.initialSubmit.intent,
    command_id: command.command_id,
  };
}

export type SessionTurnLifecycleEventType =
  | "turn_started"
  | "turn_completed"
  | "turn_failed"
  | "turn_interrupted";

export function rawEventsBelongToRuntimeStreamTarget(input: {
  rawEvents: SessionEventEnvelope[];
  agentRunTarget?: AgentRunRuntimeTarget | null;
  boundTargetKey: string | null;
}): boolean {
  const expectedTargetKey = input.agentRunTarget
    ? `${input.agentRunTarget.runId}:${input.agentRunTarget.agentId}`
    : null;
  if (!expectedTargetKey) {
    return input.rawEvents.length === 0;
  }
  return input.boundTargetKey === expectedTargetKey;
}

export function toExecutorConfigSource(
  defaults: ProjectAgentExecutor | TaskSessionExecutorSummary | ConversationEffectiveExecutorConfigView | null | undefined,
): ExecutorConfigSource | null {
  if (!defaults) return null;
  const source: ExecutorConfigSource = {};
  if (defaults.executor) source.executor = defaults.executor;
  if (defaults.provider_id) source.providerId = defaults.provider_id;
  if (defaults.model_id) source.modelId = defaults.model_id;
  if (defaults.thinking_level) source.thinkingLevel = defaults.thinking_level;
  return Object.keys(source).length === 0 ? null : source;
}

function normalizeExecutorToken(raw: string): string {
  return raw.trim().replace(/[-\s]+/g, "_").toUpperCase();
}

export function resolveExecutorFromHint(
  hint: string | null | undefined,
  executors: Array<{ id: string }>,
): string | null {
  const trimmed = (hint ?? "").trim();
  if (!trimmed) return null;
  const exact = executors.find((item) => item.id === trimmed);
  if (exact) return exact.id;
  const normalized = normalizeExecutorToken(trimmed);
  const matched = executors.find((item) => normalizeExecutorToken(item.id) === normalized);
  return matched?.id ?? trimmed;
}

function isTurnTerminalType(value: unknown): value is Exclude<SessionTurnLifecycleEventType, "turn_started"> {
  return value === "turn_completed" ||
    value === "turn_failed" ||
    value === "turn_interrupted";
}

export function extractTurnLifecycleEventType(event: BackboneEvent): SessionTurnLifecycleEventType | null {
  if (event.type === "turn_started" || event.type === "turn_completed") {
    return event.type;
  }
  if (
    event.type !== "platform" ||
    event.payload.kind !== "session_meta_update" ||
    event.payload.data.key !== "turn_terminal"
  ) {
    return null;
  }
  const value = event.payload.data.value;
  if (value === null || typeof value !== "object" || Array.isArray(value)) {
    return null;
  }
  const terminalType = (value as { terminal_type?: unknown }).terminal_type;
  return isTurnTerminalType(terminalType) ? terminalType : null;
}

export function collectRenderableSystemEvents(
  rawEvents: SessionEventEnvelope[],
  afterSeq: number,
): {
  items: Array<{ eventSeq: number; eventType: string; event: BackboneEvent }>;
  lastSeenSeq: number;
} {
  const items: Array<{ eventSeq: number; eventType: string; event: BackboneEvent }> = [];
  let lastSeenSeq = afterSeq;

  for (const event of rawEvents) {
    if (event.event_seq <= afterSeq) {
      continue;
    }
    lastSeenSeq = Math.max(lastSeenSeq, event.event_seq);
    const bbEvent = event.notification.event;
    if (bbEvent.type !== "platform") {
      continue;
    }
    const eventType = extractPlatformEventType(bbEvent);
    if (!eventType) {
      continue;
    }
    if (!shouldNotifyRenderableSystemEvent(bbEvent)) {
      continue;
    }
    items.push({
      eventSeq: event.event_seq,
      eventType,
      event: bbEvent,
    });
  }

  return { items, lastSeenSeq };
}

export const collectNewSystemEvents = collectRenderableSystemEvents;
