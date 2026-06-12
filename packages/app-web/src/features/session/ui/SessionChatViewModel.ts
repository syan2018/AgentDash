import type { BackboneEvent } from "../../../generated/backbone-protocol";
import type { ConversationEffectiveExecutorConfigView } from "../../../generated/project-agent-contracts";
import type { ExecutorConfigSource } from "../../executor-selector/model/types";
import type { TaskSessionExecutorSummary } from "../../../types/context";
import type { ProjectAgentExecutor } from "../../../types";
import type { SessionEventEnvelope } from "../model/types";
import { extractPlatformEventType } from "../model/platformEvent";
import { shouldNotifyRenderableSystemEvent } from "../model/systemEventPolicy";

export function toExecutorConfigSource(
  defaults: ProjectAgentExecutor | TaskSessionExecutorSummary | ConversationEffectiveExecutorConfigView | null | undefined,
): ExecutorConfigSource | null {
  if (!defaults) return null;
  const source: ExecutorConfigSource = {};
  if (defaults.executor) source.executor = defaults.executor;
  if (defaults.provider_id) source.providerId = defaults.provider_id;
  if (defaults.model_id) source.modelId = defaults.model_id;
  if (defaults.thinking_level) source.thinkingLevel = defaults.thinking_level;
  if (defaults.permission_policy) source.permissionPolicy = defaults.permission_policy;
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

function isCompactionSummaryFrame(event: BackboneEvent): boolean {
  if (
    event.type !== "platform" ||
    event.payload.kind !== "session_meta_update" ||
    event.payload.data.key !== "context_frame"
  ) {
    return false;
  }
  const value = event.payload.data.value;
  return value !== null && typeof value === "object" && !Array.isArray(value) &&
    value.kind === "compaction_summary";
}

function isProjectionRefreshEvent(event: BackboneEvent): boolean {
  if (event.type === "turn_completed") {
    return true;
  }
  if (event.type !== "platform") {
    return false;
  }
  return extractPlatformEventType(event) === "context_compacted" ||
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
