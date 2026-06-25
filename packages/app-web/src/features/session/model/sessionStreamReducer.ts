import type { BackboneEvent, AgentDashThreadItem } from "../../../generated/backbone-protocol";
import type {
  SessionDisplayEntry,
  SessionEventEnvelope,
  SessionItemFreshness,
  TimelineOrder,
  TokenUsageInfo,
} from "./types";
import { extractTextFromUserInputs, extractTokenUsageFromEvent } from "./types";
import { isRecord } from "./platformEvent";
import { parseContextFrame } from "./contextFrame";

export interface SessionStreamState {
  entries: SessionDisplayEntry[];
  rawEvents: SessionEventEnvelope[];
  tokenUsage: TokenUsageInfo | null;
  providerWaitingSeqs: ReadonlyMap<string, number>;
  lastAppliedSeq: number;
  /**
   * 最近应用的 ephemeral 事件的 ephemeral_seq（承载于 event.event_seq）。
   * 用于整页刷新 / 断线重连时按 seq 去重 in-flight 进度态：
   * - 整页刷新：新 state lastEphemeralSeq=0 → 回放服务端 buffer 全部累积 delta；
   * - 断线重连复用 state：lastEphemeralSeq=k → 只应用 seq>k 的，不重复累加。
   * ephemeral seq 与 durable event_seq 不同语义，互不干扰。
   */
  lastEphemeralSeq: number;
}

export function createInitialStreamState(initialEntries: SessionDisplayEntry[]): SessionStreamState {
  const lastAppliedSeq = initialEntries.reduce((max, entry) => Math.max(max, entry.eventSeq), 0);
  return {
    entries: initialEntries,
    rawEvents: [],
    tokenUsage: null,
    providerWaitingSeqs: new Map(),
    lastAppliedSeq,
    lastEphemeralSeq: 0,
  };
}

function threadItemId(item: AgentDashThreadItem): string {
  return item.id;
}

function getItemIdFromEvent(event: BackboneEvent): string | undefined {
  switch (event.type) {
    case "item_started":
    case "item_updated":
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

function makeTimelineOrder(event: SessionEventEnvelope, bbEvent: BackboneEvent): TimelineOrder {
  if (!event.ephemeral) {
    return { kind: "durable", seq: event.event_seq };
  }

  const itemId = getItemIdFromEvent(bbEvent);
  if (itemId) {
    return {
      kind: "anchored_progress",
      anchorId: `item:${itemId}`,
      progressSeq: event.event_seq,
    };
  }

  return {
    kind: "local_progress",
    receivedOrdinal: event.event_seq,
    progressSeq: event.event_seq,
  };
}

const ITEM_FRESHNESS_RANK: Record<SessionItemFreshness, number> = {
  started: 1,
  progress: 2,
  completed: 3,
};

function freshnessForEvent(event: BackboneEvent): SessionItemFreshness | undefined {
  switch (event.type) {
    case "item_started":
      return "started";
    case "item_updated":
    case "command_output_delta":
    case "file_change_delta":
    case "mcp_tool_call_progress":
    case "agent_message_delta":
    case "reasoning_text_delta":
    case "reasoning_summary_delta":
      return "progress";
    case "item_completed":
      return "completed";
    default:
      return undefined;
  }
}

function isFreshEnough(
  existing: SessionDisplayEntry,
  incomingFreshness: SessionItemFreshness,
): boolean {
  const existingFreshness = existing.itemFreshness;
  if (!existingFreshness) {
    return true;
  }
  return ITEM_FRESHNESS_RANK[incomingFreshness] >= ITEM_FRESHNESS_RANK[existingFreshness];
}

function mergeEntryMetadata(
  existing: SessionDisplayEntry,
  event: SessionEventEnvelope,
  bbEvent: BackboneEvent,
  incomingFreshness: SessionItemFreshness | undefined,
): SessionDisplayEntry {
  const timelineOrder = makeTimelineOrder(event, bbEvent);
  const existingFreshness = existing.itemFreshness;
  const nextFreshness =
    incomingFreshness && (!existingFreshness || isFreshEnough(existing, incomingFreshness))
      ? incomingFreshness
      : existingFreshness;

  return {
    ...existing,
    timestamp: event.committed_at_ms ?? event.occurred_at_ms ?? existing.timestamp,
    eventSeq: event.ephemeral && existing.timelineOrder?.kind === "durable"
      ? existing.eventSeq
      : event.event_seq,
    timelineOrder: event.ephemeral && existing.timelineOrder?.kind === "durable"
      ? existing.timelineOrder
      : timelineOrder,
    progressSeq: event.ephemeral ? event.event_seq : existing.progressSeq,
    itemFreshness: nextFreshness,
  };
}

function withEntryMetadata(
  entry: SessionDisplayEntry,
  event: SessionEventEnvelope,
  bbEvent: BackboneEvent,
): SessionDisplayEntry {
  return {
    ...entry,
    timelineOrder: makeTimelineOrder(event, bbEvent),
    progressSeq: event.ephemeral ? event.event_seq : undefined,
    itemFreshness: freshnessForEvent(bbEvent),
  };
}

function getCommandAggregatedOutput(item: AgentDashThreadItem): string | null {
  if (item.type !== "commandExecution" && item.type !== "shellExec") {
    return null;
  }
  return item.aggregatedOutput;
}

function isWillRetryErrorEvent(event: BackboneEvent): boolean {
  return event.type === "error" && event.payload.willRetry === true;
}

function readStringField(record: Record<string, unknown>, key: string): string | undefined {
  const value = record[key];
  return typeof value === "string" && value.trim().length > 0 ? value : undefined;
}

function eventTurnId(event: SessionEventEnvelope): string | undefined {
  return event.turn_id ?? event.notification.trace.turnId ?? undefined;
}

function extractProviderAttemptStatus(event: SessionEventEnvelope): { turnId?: string; phase: string } | null {
  const bbEvent = event.notification.event;
  if (bbEvent.type !== "platform" || !isRecord(bbEvent.payload)) {
    return null;
  }

  const platform: Record<string, unknown> = bbEvent.payload;
  const kind = readStringField(platform, "kind");
  if (kind !== "provider_attempt_status" || !isRecord(platform.data)) {
    return null;
  }

  const phase = readStringField(platform.data, "phase");
  if (!phase) {
    return null;
  }

  return {
    turnId: readStringField(platform.data, "turn_id") ?? eventTurnId(event),
    phase,
  };
}

function extractTerminalTurnId(event: SessionEventEnvelope): string | null {
  const bbEvent = event.notification.event;
  if (bbEvent.type === "turn_completed") {
    return bbEvent.payload.turn.id;
  }

  if (
    bbEvent.type !== "platform" ||
    bbEvent.payload.kind !== "session_meta_update" ||
    bbEvent.payload.data.key !== "turn_terminal" ||
    !isRecord(bbEvent.payload.data.value)
  ) {
    return null;
  }

  return readStringField(bbEvent.payload.data.value, "turn_id") ?? eventTurnId(event) ?? null;
}

function updateProviderWaitingSeqs(
  current: ReadonlyMap<string, number>,
  event: SessionEventEnvelope,
): ReadonlyMap<string, number> {
  const terminalTurnId = extractTerminalTurnId(event);
  if (terminalTurnId) {
    if (!current.has(terminalTurnId)) return current;
    const next = new Map(current);
    next.delete(terminalTurnId);
    return next;
  }

  const status = extractProviderAttemptStatus(event);
  if (!status?.turnId) {
    return current;
  }

  const next = new Map(current);
  if (status.phase === "connected_waiting_first_delta") {
    next.set(status.turnId, event.event_seq);
  } else {
    next.delete(status.turnId);
  }
  return next;
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
    timelineOrder: makeTimelineOrder(event, bbEvent),
    progressSeq: event.ephemeral ? event.event_seq : undefined,
    itemFreshness: freshnessForEvent(bbEvent),
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

type AssistantDeltaKind =
  | "agent_message_delta"
  | "reasoning_text_delta"
  | "reasoning_summary_delta";

/** 为 hydrate 场景合成一个 delta 事件，使终态助手正文 / reasoning 仍渲染为助手/思考卡。 */
function synthesizeAssistantDeltaEvent(
  kind: AssistantDeltaKind,
  event: SessionEventEnvelope,
  itemId: string,
  text: string,
): BackboneEvent {
  const base = {
    threadId: event.notification.sessionId,
    turnId: event.turn_id ?? "",
    itemId,
    delta: text,
  };
  if (kind === "reasoning_text_delta") {
    return { type: kind, payload: { ...base, contentIndex: 0 } };
  }
  if (kind === "reasoning_summary_delta") {
    return { type: kind, payload: { ...base, summaryIndex: 0 } };
  }
  return { type: kind, payload: base };
}

/**
 * 终态助手正文 / reasoning（来自 turn 收尾 ItemCompleted(AgentMessage|Reasoning)）并入对应 delta 气泡。
 * - live：命中已存在的 delta 条目，用终态文本 finalize（权威覆盖累积），isStreaming=false。
 * - hydrate（Step 1 后只有终态、无 delta）：合成同 id 的 delta 条目渲染气泡。
 * 不新建 `item:` 卡片，避免与流式气泡双渲染。
 */
function finalizeAssistantDelta(
  entries: SessionDisplayEntry[],
  event: SessionEventEnvelope,
  kind: AssistantDeltaKind,
  itemId: string,
  text: string,
): SessionDisplayEntry[] {
  if (!text) {
    return entries;
  }
  // delta 气泡的 entry id 与该 delta 的 itemId 同源（buildEntryId 优先用 getItemIdFromEvent）。
  // 终态 item 的 id 与 delta 的 itemId 同为 synth_item_id(turn,entry,"msg"|"reason")，故 target 一致。
  const targetId = `item:${itemId}`;

  for (let i = entries.length - 1; i >= 0; i -= 1) {
    const existing = entries[i];
    if (existing && existing.id === targetId) {
      const next = [...entries];
      const merged = mergeEntryMetadata(existing, event, event.notification.event, "completed");
      // 保留既有 delta event（渲染分发依赖 event.type），仅 finalize 文本与流式标记。
      next[i] = { ...merged, accumulatedText: text, isStreaming: false };
      return next;
    }
  }

  // hydrate：无 delta 气泡，合成同 id 的 delta 条目以渲染助手 / 思考卡。
  const syntheticEvent = synthesizeAssistantDeltaEvent(kind, event, itemId, text);
  return [
    ...entries,
    withEntryMetadata({
      id: targetId,
      sessionId: event.notification.sessionId,
      timestamp: event.committed_at_ms ?? event.occurred_at_ms ?? Date.now(),
      eventSeq: event.event_seq,
      event: syntheticEvent,
      turnId: event.turn_id ?? undefined,
      entryIndex: event.entry_index ?? undefined,
      accumulatedText: text,
      isStreaming: false,
    }, event, event.notification.event),
  ];
}

function applyEventToEntries(prev: SessionDisplayEntry[], event: SessionEventEnvelope): SessionDisplayEntry[] {
  const bbEvent: BackboneEvent = event.notification.event;

  if (bbEvent.type === "agent_message_delta") {
    const entryId = buildEntryId(event, bbEvent);
    for (let i = prev.length - 1; i >= 0; i -= 1) {
      const existing = prev[i];
      if (existing && existing.id === entryId) {
        // P1-b 防御：该气泡已被终态 finalize（isStreaming=false），跳过在途旧 delta，
        // 防止后端剪枝前已在网络途中的 ephemeral delta append 脏化已 final 正文。
        if (existing.isStreaming === false) {
          return prev;
        }
        const accumulated = (existing.accumulatedText ?? "") + bbEvent.payload.delta;
        const next = [...prev];
        next[i] = {
          ...mergeEntryMetadata(existing, event, bbEvent, "progress"),
          event: bbEvent,
          accumulatedText: accumulated,
          isStreaming: true,
        };
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
        if (existing.isStreaming === false) {
          return prev;
        }
        const accumulated = (existing.accumulatedText ?? "") + bbEvent.payload.delta;
        const next = [...prev];
        next[i] = {
          ...mergeEntryMetadata(existing, event, bbEvent, "progress"),
          event: bbEvent,
          accumulatedText: accumulated,
        };
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
        if (existing.isStreaming === false) {
          return prev;
        }
        const accumulated = (existing.accumulatedText ?? "") + bbEvent.payload.delta;
        const next = [...prev];
        next[i] = {
          ...mergeEntryMetadata(existing, event, bbEvent, "progress"),
          event: bbEvent,
          accumulatedText: accumulated,
        };
        return next;
      }
    }
    return [...prev, { ...makeDisplayEntry(event, bbEvent), accumulatedText: bbEvent.payload.delta }];
  }

  if (bbEvent.type === "item_started" || bbEvent.type === "item_updated") {
    const entryId = buildEntryId(event, bbEvent);
    const incomingFreshness = freshnessForEvent(bbEvent);
    for (let i = prev.length - 1; i >= 0; i -= 1) {
      const existing = prev[i];
      if (existing && existing.id === entryId) {
        const next = [...prev];
        const merged = mergeEntryMetadata(existing, event, bbEvent, incomingFreshness);
        next[i] = incomingFreshness && isFreshEnough(existing, incomingFreshness)
          ? { ...merged, event: bbEvent }
          : merged;
        return next;
      }
    }
    return [...prev, makeDisplayEntry(event, bbEvent)];
  }

  if (bbEvent.type === "item_completed") {
    const finalItem = bbEvent.payload.item;
    // 终态助手正文 / reasoning 并入 delta 气泡，不走工具卡路径。
    if (finalItem.type === "agentMessage") {
      return finalizeAssistantDelta(prev, event, "agent_message_delta", finalItem.id, finalItem.text);
    }
    if (finalItem.type === "reasoning") {
      let next = prev;
      const contentText = finalItem.content.join("");
      if (contentText) {
        next = finalizeAssistantDelta(next, event, "reasoning_text_delta", finalItem.id, contentText);
      }
      const summaryText = finalItem.summary.join("");
      if (summaryText) {
        next = finalizeAssistantDelta(next, event, "reasoning_summary_delta", finalItem.id, summaryText);
      }
      return next;
    }

    const entryId = buildEntryId(event, bbEvent);
    const finalCommandOutput = getCommandAggregatedOutput(bbEvent.payload.item);
    for (let i = prev.length - 1; i >= 0; i -= 1) {
      const existing = prev[i];
      if (existing && existing.id === entryId) {
        const next = [...prev];
        const merged = mergeEntryMetadata(existing, event, bbEvent, "completed");
        next[i] = {
          ...merged,
          event: bbEvent,
          accumulatedText: finalCommandOutput ?? existing.accumulatedText,
          isStreaming: false,
          isPendingApproval: false,
        };
        return next;
      }
    }
    return [
      ...prev,
      {
        ...makeDisplayEntry(event, bbEvent),
        accumulatedText: finalCommandOutput ?? undefined,
        isStreaming: false,
      },
    ];
  }

  if (bbEvent.type === "command_output_delta" || bbEvent.type === "file_change_delta" ||
      bbEvent.type === "mcp_tool_call_progress") {
    const itemId = bbEvent.payload.itemId;
    const targetId = `item:${itemId}`;
    for (let i = prev.length - 1; i >= 0; i -= 1) {
      const existing = prev[i];
      if (existing && existing.id === targetId) {
        if (existing.itemFreshness === "completed") {
          return prev;
        }
        const deltaText = bbEvent.type === "mcp_tool_call_progress"
          ? bbEvent.payload.message
          : bbEvent.payload.delta;
        const accumulated = (existing.accumulatedText ?? "") + deltaText;
        const next = [...prev];
        next[i] = {
          ...mergeEntryMetadata(existing, event, bbEvent, "progress"),
          accumulatedText: accumulated,
        };
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
        next[i] = {
          ...mergeEntryMetadata(existing, event, bbEvent, undefined),
          event: bbEvent,
          accumulatedText: text,
        };
        return next;
      }
    }
    return [...prev, { ...makeDisplayEntry(event, bbEvent), accumulatedText: text }];
  }

  if (bbEvent.type === "error") {
    if (isWillRetryErrorEvent(bbEvent)) {
      return prev;
    }
    return [...prev, makeDisplayEntry(event, bbEvent)];
  }

  if (bbEvent.type === "platform") {
    const platform = bbEvent.payload;

    if (platform.kind === "provider_attempt_status") {
      return prev;
    }

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

function orderIncomingEvents(incomingEvents: SessionEventEnvelope[]): SessionEventEnvelope[] {
  const durableEvents = incomingEvents
    .filter((event) => !event.ephemeral)
    .sort((a, b) => a.event_seq - b.event_seq);
  const ephemeralEvents = incomingEvents
    .filter((event) => event.ephemeral)
    .sort((a, b) => a.event_seq - b.event_seq);
  let durableIndex = 0;
  let ephemeralIndex = 0;

  return incomingEvents.map((event) => {
    if (event.ephemeral) {
      const ordered = ephemeralEvents[ephemeralIndex];
      ephemeralIndex += 1;
      return ordered ?? event;
    }
    const ordered = durableEvents[durableIndex];
    durableIndex += 1;
    return ordered ?? event;
  });
}

export function reduceStreamState(
  prev: SessionStreamState,
  incomingEvents: SessionEventEnvelope[],
): SessionStreamState {
  if (incomingEvents.length === 0) {
    return prev;
  }

  let entries = prev.entries;
  let rawEvents = prev.rawEvents;
  let tokenUsage = prev.tokenUsage;
  let providerWaitingSeqs = prev.providerWaitingSeqs;
  let lastAppliedSeq = prev.lastAppliedSeq;
  let lastEphemeralSeq = prev.lastEphemeralSeq;

  // 同一 lane 内分别按各自 seq 去重/排序，但保留 incoming batch 的 durable/ephemeral lane 位置。
  // 这样 ephemeral_seq 不会和 durable event_seq 共轴比较，也不会把整批 progress 先于 durable lifecycle 应用。
  for (const event of orderIncomingEvents(incomingEvents)) {
    if (event.ephemeral) {
      if (event.event_seq <= lastEphemeralSeq) {
        continue;
      }
      providerWaitingSeqs = updateProviderWaitingSeqs(providerWaitingSeqs, event);
      entries = applyEventToEntries(entries, event);
      lastEphemeralSeq = event.event_seq;
      continue;
    }

    if (event.event_seq <= lastAppliedSeq) {
      continue;
    }

    rawEvents = [...rawEvents, event];
    providerWaitingSeqs = updateProviderWaitingSeqs(providerWaitingSeqs, event);
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
    providerWaitingSeqs,
    lastAppliedSeq,
    lastEphemeralSeq,
  };
}

/**
 * 重置 ephemeral 去重游标（后端进程重启 / epoch 变化时调用）。
 * 后端重启后 ephemeral_seq 从 0 重来，旧 state 的高位 lastEphemeralSeq 会误跳过新 turn 的
 * live delta；epoch 变化时把游标归零，使新 epoch 的 ephemeral 流从头应用。
 * 同时清掉 live-only provider waiting 状态，避免后端重启后保留旧连接过程提示。
 * 不触碰已累积的 entries / rawEvents / durable 游标。
 */
export function resetEphemeralCursor(prev: SessionStreamState): SessionStreamState {
  if (prev.lastEphemeralSeq === 0 && prev.providerWaitingSeqs.size === 0) {
    return prev;
  }
  return { ...prev, providerWaitingSeqs: new Map(), lastEphemeralSeq: 0 };
}

export function shouldFlushStreamEventImmediately(event: SessionEventEnvelope): boolean {
  const t = event.notification.event.type;
  return t === "item_started" || t === "item_completed" || t === "approval_request";
}
