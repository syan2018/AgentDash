import type { BackboneEvent } from "../../../generated/backbone-protocol";
import type { ConversationEffectiveExecutorConfigView } from "../../../generated/project-agent-contracts";
import type { ExecutorConfigSource } from "../../executor-selector/model/types";
import type { TaskSessionExecutorSummary } from "../../../types/context";
import type { ProjectAgentExecutor } from "../../../types";
import type { SessionEventEnvelope } from "../model/types";
import type { AgentRunRuntimeTarget } from "../../../services/agentRunRuntime";
import {
  extractContextFrameValue,
  extractPlatformEventType,
  isRecord,
} from "../model/platformEvent";
import { shouldNotifyRenderableSystemEvent } from "../model/systemEventPolicy";
import type {
  SessionChatCommandModel,
  SessionChatInitialSubmit,
  SessionChatSubmitIntent,
} from "./SessionChatViewTypes";

/**
 * Snapshot hydration is not eligible for live Product side effects.
 *
 * A gap reload may advance the replay boundary while the page and its refs stay mounted, so the
 * previous live cursor must always be fenced by the latest baseline boundary.
 */
export function liveSideEffectCursor(
  previous: number | null,
  historyReplayBoundarySeq: number,
): number {
  return Math.max(previous ?? historyReplayBoundarySeq, historyReplayBoundarySeq);
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

export function isAgentRunWorkspaceActionRunning(input: {
  executionStatus: string;
}): boolean {
  return input.executionStatus === "starting_claimed" ||
    input.executionStatus === "running_active" ||
    input.executionStatus === "cancelling";
}

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

export function collectAllPlatformEvents(
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
    items.push({
      eventSeq: event.event_seq,
      eventType,
      event: bbEvent,
    });
  }

  return { items, lastSeenSeq };
}

export function collectTurnLifecycleEvents(
  rawEvents: SessionEventEnvelope[],
  afterSeq: number,
): {
  items: Array<{ eventSeq: number; eventType: SessionTurnLifecycleEventType; event: BackboneEvent }>;
  lastSeenSeq: number;
} {
  const items: Array<{ eventSeq: number; eventType: SessionTurnLifecycleEventType; event: BackboneEvent }> = [];
  let lastSeenSeq = afterSeq;

  for (const event of rawEvents) {
    if (event.event_seq <= afterSeq) {
      continue;
    }
    lastSeenSeq = Math.max(lastSeenSeq, event.event_seq);
    const bbEvent = event.notification.event;
    const eventType = extractTurnLifecycleEventType(bbEvent);
    if (!eventType) {
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

function isCompactionSummaryFrame(event: BackboneEvent): boolean {
  return extractContextFrameValue(event)?.kind === "compaction_summary";
}

function isSessionRewindRefreshEvent(event: BackboneEvent): boolean {
  if (event.type !== "platform") {
    return false;
  }
  if (isRecord(event.payload)) {
    const kind = typeof event.payload.kind === "string" ? event.payload.kind : null;
    if (kind === "session_rewound") {
      return true;
    }
  }
  const eventType = extractPlatformEventType(event);
  return eventType === "session_rewound" ||
    eventType === "session_rebuilt" ||
    eventType === "turn_discarded" ||
    eventType === "projection_invalidated";
}

function isProjectionRefreshEvent(event: BackboneEvent): boolean {
  const turnLifecycleType = extractTurnLifecycleEventType(event);
  if (turnLifecycleType && turnLifecycleType !== "turn_started") {
    return true;
  }
  if (event.type !== "platform") {
    return false;
  }
  return extractPlatformEventType(event) === "context_compacted" ||
    isSessionRewindRefreshEvent(event) ||
    isCompactionSummaryFrame(event);
}

export function computeProjectionRefreshKey(rawEvents: SessionEventEnvelope[]): number {
  let refreshKey = 0;
  for (const event of rawEvents) {
    if (isProjectionRefreshEvent(event.notification.event)) {
      refreshKey = Math.max(refreshKey, event.event_seq);
    }
  }
  return refreshKey;
}
