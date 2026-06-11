import type { BackboneEvent, AgentDashThreadItem } from "../../../generated/backbone-protocol";
import type {
  SessionDisplayEntry,
  SessionEventEnvelope,
  TokenUsageInfo,
} from "./types";
import { extractTextFromUserInputs, extractTokenUsageFromEvent } from "./types";
import { isRecord } from "./platformEvent";
import { parseContextFrame } from "./contextFrame";

export interface SessionStreamState {
  entries: SessionDisplayEntry[];
  rawEvents: SessionEventEnvelope[];
  tokenUsage: TokenUsageInfo | null;
  lastAppliedSeq: number;
}

export function createInitialStreamState(initialEntries: SessionDisplayEntry[]): SessionStreamState {
  const lastAppliedSeq = initialEntries.reduce((max, entry) => Math.max(max, entry.eventSeq), 0);
  return {
    entries: initialEntries,
    rawEvents: [],
    tokenUsage: null,
    lastAppliedSeq,
  };
}

function threadItemId(item: AgentDashThreadItem): string {
  return item.id;
}

function getItemIdFromEvent(event: BackboneEvent): string | undefined {
  switch (event.type) {
    case "item_started":
    case "item_completed":
      return threadItemId(event.payload.item);
    case "command_output_delta":
    case "file_change_delta":
    case "mcp_tool_call_progress":
    case "agent_message_delta":
    case "reasoning_text_delta":
    case "reasoning_summary_delta":
    case "plan_delta":
      return event.payload.itemId;
    default:
      return undefined;
  }
}

function buildEntryId(event: SessionEventEnvelope, bbEvent: BackboneEvent): string {
  const itemId = getItemIdFromEvent(bbEvent);
  if (itemId) {
    return `item:${itemId}`;
  }

  const turnId = event.turn_id;
  const entryIndex = event.entry_index;

  if (bbEvent.type === "agent_message_delta" || bbEvent.type === "reasoning_text_delta" ||
      bbEvent.type === "reasoning_summary_delta") {
    if (turnId && entryIndex != null) {
      return `delta:${bbEvent.type}:${turnId}:${entryIndex}`;
    }
    const payloadItemId = bbEvent.payload.itemId;
    if (payloadItemId) {
      return `delta:${bbEvent.type}:${payloadItemId}`;
    }
  }

  if (bbEvent.type === "user_input_submitted") {
    return `user-input:${bbEvent.payload.turnId}:${bbEvent.payload.itemId}`;
  }

  return `event:${event.event_seq}`;
}

export function makeDisplayEntry(event: SessionEventEnvelope, bbEvent: BackboneEvent): SessionDisplayEntry {
  const entry: SessionDisplayEntry = {
    id: buildEntryId(event, bbEvent),
    sessionId: event.notification.sessionId,
    timestamp: event.committed_at_ms ?? event.occurred_at_ms ?? Date.now(),
    eventSeq: event.event_seq,
    event: bbEvent,
    turnId: event.turn_id ?? undefined,
    entryIndex: event.entry_index ?? undefined,
  };

  if (
    bbEvent.type === "platform" &&
    bbEvent.payload.kind === "session_meta_update" &&
    bbEvent.payload.data.key === "context_frame" &&
    isRecord(bbEvent.payload.data.value)
  ) {
    const contextFrame = parseContextFrame(bbEvent.payload.data.value);
    if (contextFrame) {
      return { ...entry, contextFrame };
    }
  }

  return entry;
}

function applyEventToEntries(prev: SessionDisplayEntry[], event: SessionEventEnvelope): SessionDisplayEntry[] {
  const bbEvent: BackboneEvent = event.notification.event;

  if (bbEvent.type === "agent_message_delta") {
    const entryId = buildEntryId(event, bbEvent);
    for (let i = prev.length - 1; i >= 0; i -= 1) {
      const existing = prev[i];
      if (existing && existing.id === entryId) {
        const accumulated = (existing.accumulatedText ?? "") + bbEvent.payload.delta;
        const next = [...prev];
        next[i] = { ...existing, eventSeq: event.event_seq, event: bbEvent, accumulatedText: accumulated, isStreaming: true };
        return next;
      }
    }
    return [...prev, { ...makeDisplayEntry(event, bbEvent), accumulatedText: bbEvent.payload.delta, isStreaming: true }];
  }

  if (bbEvent.type === "reasoning_text_delta") {
    const entryId = buildEntryId(event, bbEvent);
    for (let i = prev.length - 1; i >= 0; i -= 1) {
      const existing = prev[i];
      if (existing && existing.id === entryId) {
        const accumulated = (existing.accumulatedText ?? "") + bbEvent.payload.delta;
        const next = [...prev];
        next[i] = { ...existing, eventSeq: event.event_seq, event: bbEvent, accumulatedText: accumulated };
        return next;
      }
    }
    return [...prev, { ...makeDisplayEntry(event, bbEvent), accumulatedText: bbEvent.payload.delta }];
  }

  if (bbEvent.type === "reasoning_summary_delta") {
    const entryId = buildEntryId(event, bbEvent);
    for (let i = prev.length - 1; i >= 0; i -= 1) {
      const existing = prev[i];
      if (existing && existing.id === entryId) {
        const accumulated = (existing.accumulatedText ?? "") + bbEvent.payload.delta;
        const next = [...prev];
        next[i] = { ...existing, eventSeq: event.event_seq, event: bbEvent, accumulatedText: accumulated };
        return next;
      }
    }
    return [...prev, { ...makeDisplayEntry(event, bbEvent), accumulatedText: bbEvent.payload.delta }];
  }

  if (bbEvent.type === "item_started") {
    const entryId = buildEntryId(event, bbEvent);
    for (let i = prev.length - 1; i >= 0; i -= 1) {
      const existing = prev[i];
      if (existing && existing.id === entryId) {
        const next = [...prev];
        next[i] = { ...existing, eventSeq: event.event_seq, event: bbEvent };
        return next;
      }
    }
    return [...prev, makeDisplayEntry(event, bbEvent)];
  }

  if (bbEvent.type === "item_completed") {
    const entryId = buildEntryId(event, bbEvent);
    for (let i = prev.length - 1; i >= 0; i -= 1) {
      const existing = prev[i];
      if (existing && existing.id === entryId) {
        const next = [...prev];
        next[i] = {
          ...existing,
          eventSeq: event.event_seq,
          event: bbEvent,
          isStreaming: false,
          isPendingApproval: false,
        };
        return next;
      }
    }
    return [...prev, { ...makeDisplayEntry(event, bbEvent), isStreaming: false }];
  }

  if (bbEvent.type === "command_output_delta" || bbEvent.type === "file_change_delta" ||
      bbEvent.type === "mcp_tool_call_progress") {
    const itemId = bbEvent.payload.itemId;
    const targetId = `item:${itemId}`;
    for (let i = prev.length - 1; i >= 0; i -= 1) {
      const existing = prev[i];
      if (existing && existing.id === targetId) {
        const deltaText = bbEvent.type === "mcp_tool_call_progress"
          ? bbEvent.payload.message
          : bbEvent.payload.delta;
        const accumulated = (existing.accumulatedText ?? "") + deltaText;
        const next = [...prev];
        next[i] = { ...existing, eventSeq: event.event_seq, accumulatedText: accumulated };
        return next;
      }
    }
    return prev;
  }

  if (bbEvent.type === "turn_started" || bbEvent.type === "turn_completed") {
    return prev;
  }

  if (bbEvent.type === "turn_plan_updated") {
    return [...prev, makeDisplayEntry(event, bbEvent)];
  }

  if (bbEvent.type === "plan_delta") {
    return prev;
  }

  if (bbEvent.type === "token_usage_updated") {
    return prev;
  }

  if (bbEvent.type === "approval_request") {
    return [...prev, { ...makeDisplayEntry(event, bbEvent), isPendingApproval: true }];
  }

  if (bbEvent.type === "user_input_submitted") {
    const entryId = buildEntryId(event, bbEvent);
    const text = extractTextFromUserInputs(bbEvent.payload.content);
    for (let i = prev.length - 1; i >= 0; i -= 1) {
      const existing = prev[i];
      if (existing && existing.id === entryId) {
        const next = [...prev];
        next[i] = { ...existing, eventSeq: event.event_seq, event: bbEvent, accumulatedText: text };
        return next;
      }
    }
    return [...prev, { ...makeDisplayEntry(event, bbEvent), accumulatedText: text }];
  }

  if (bbEvent.type === "error") {
    return [...prev, makeDisplayEntry(event, bbEvent)];
  }

  if (bbEvent.type === "platform") {
    const platform = bbEvent.payload;

    if (platform.kind === "terminal_output" || platform.kind === "terminal_state_changed") {
      return prev;
    }

    if (platform.kind === "session_meta_update") {
      const key = platform.data.key;
      if (key === "session_meta_updated" || key === "acp_passthrough") {
        return prev;
      }
      return [...prev, makeDisplayEntry(event, bbEvent)];
    }

    return [...prev, makeDisplayEntry(event, bbEvent)];
  }

  if (bbEvent.type === "thread_status_changed" || bbEvent.type === "executor_context_compacted" ||
      bbEvent.type === "turn_diff_updated") {
    return prev;
  }

  return [...prev, makeDisplayEntry(event, bbEvent)];
}

export function reduceStreamState(
  prev: SessionStreamState,
  incomingEvents: SessionEventEnvelope[],
): SessionStreamState {
  if (incomingEvents.length === 0) {
    return prev;
  }

  const normalized = [...incomingEvents].sort((a, b) => a.event_seq - b.event_seq);

  let entries = prev.entries;
  let rawEvents = prev.rawEvents;
  let tokenUsage = prev.tokenUsage;
  let lastAppliedSeq = prev.lastAppliedSeq;

  for (const event of normalized) {
    if (event.event_seq <= lastAppliedSeq) {
      continue;
    }
    rawEvents = [...rawEvents, event];
    entries = applyEventToEntries(entries, event);
    const usage = extractTokenUsageFromEvent(event.notification.event);
    if (usage) {
      tokenUsage = tokenUsage ? { ...tokenUsage, ...usage } : usage;
    }
    lastAppliedSeq = event.event_seq;
  }

  return {
    entries,
    rawEvents,
    tokenUsage,
    lastAppliedSeq,
  };
}

export function shouldFlushStreamEventImmediately(event: SessionEventEnvelope): boolean {
  const t = event.notification.event.type;
  return t === "item_started" || t === "item_completed" || t === "approval_request";
}
