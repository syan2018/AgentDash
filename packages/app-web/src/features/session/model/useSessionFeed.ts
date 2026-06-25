/**
 * 会话管理 Hook
 *
 * 整合流管理和条目聚合逻辑。
 * 暴露 displayItems（聚合后）、rawEntries、tokenUsage 等供 UI 使用。
 */

import { useMemo } from "react";
import { useDebugPrefs } from "../../../hooks/use-debug-prefs";
import { useSessionStream } from "./useSessionStream";
import type { BackboneEvent, AgentDashThreadItem } from "../../../generated/backbone-protocol";
import { parseBoundedOutputText } from "./boundedOutput";
import { getPlatformEventPolicy } from "./systemEventPolicy";
import { isRecord } from "./platformEvent";
import { isToolBurstEligible } from "./threadItemKind";
import type {
  AggregatedContextFrameGroup,
  AggregatedThinkingGroup,
  SessionDisplayEntry,
  SessionDisplayItem,
  AggregatedEntryGroup,
  SessionEventEnvelope,
  TokenUsageInfo,
} from "./types";

export interface UseSessionFeedOptions {
  sessionId: string;
  endpoint?: string;
  enableAggregation?: boolean;
  enabled?: boolean;
}

export interface UseSessionFeedResult {
  displayItems: SessionDisplayItem[];
  turnSegments: TurnSegment[];
  rawEntries: SessionDisplayEntry[];
  rawEvents: SessionEventEnvelope[];
  isConnected: boolean;
  isLoading: boolean;
  isReceiving: boolean;
  error: Error | null;
  reconnect: () => void;
  close: () => void;
  sendCancel: () => Promise<void>;
  streamingEntryId: string | null;
  tokenUsage: TokenUsageInfo | null;
}

function extractThreadItem(event: BackboneEvent): AgentDashThreadItem | null {
  if (event.type === "item_started" || event.type === "item_updated" || event.type === "item_completed") {
    return event.payload.item;
  }
  return null;
}

function isToolBurstEvent(event: BackboneEvent): boolean {
  const item = extractThreadItem(event);
  return item != null && isToolBurstEligible(item);
}

function itemTextOutputs(item: AgentDashThreadItem): string[] {
  switch (item.type) {
    case "commandExecution":
    case "shellExec":
      return item.aggregatedOutput ? [item.aggregatedOutput] : [];
    case "dynamicToolCall":
    case "fsRead":
    case "fsGrep":
    case "fsGlob":
      return (item.contentItems ?? [])
        .filter((contentItem) => contentItem.type === "inputText")
        .map((contentItem) => contentItem.text);
    case "mcpToolCall": {
      const texts: string[] = [];
      for (const contentItem of item.result?.content ?? []) {
        if (contentItem == null || typeof contentItem !== "object" || Array.isArray(contentItem)) {
          continue;
        }
        const type = contentItem.type;
        const text = contentItem.text;
        if (type === "text" && typeof text === "string") {
          texts.push(text);
        }
      }
      return texts;
    }
    default:
      return [];
  }
}

function hasBoundedOutputEntry(entry: SessionDisplayEntry): boolean {
  if (parseBoundedOutputText(entry.accumulatedText)) {
    return true;
  }
  const item = extractThreadItem(entry.event);
  if (!item) return false;
  return itemTextOutputs(item).some((text) => parseBoundedOutputText(text) != null);
}

function isContextFrameEvent(event: BackboneEvent): boolean {
  return (
    event.type === "platform" &&
    event.payload.kind === "session_meta_update" &&
    event.payload.data.key === "context_frame"
  );
}

function isWillRetryErrorEvent(event: BackboneEvent): boolean {
  return event.type === "error" && event.payload.willRetry === true;
}

type EntryClassification =
  | "tool_like"
  | "tool_single"
  | "thinking"
  | "hard_boundary"
  | "neutral";

function isEffectivelyEmptyTextEntry(entry: SessionDisplayEntry): boolean {
  const event = entry.event;
  if (
    event.type !== "agent_message_delta" &&
    event.type !== "reasoning_text_delta" &&
    event.type !== "reasoning_summary_delta"
  ) {
    return false;
  }

  const text = entry.accumulatedText ?? event.payload.delta ?? "";
  return text.trim().length === 0;
}

interface AggregateEntriesOptions {
  includeVerboseEvents?: boolean;
}

function classifyEntry(
  entry: SessionDisplayEntry,
  options: AggregateEntriesOptions = {},
): EntryClassification {
  const event = entry.event;
  if (isToolBurstEvent(event)) {
    if (hasBoundedOutputEntry(entry)) return "tool_single";
    return "tool_like";
  }

  if (event.type === "turn_started" || event.type === "turn_completed") {
    return "neutral";
  }

  if (
    event.type === "reasoning_text_delta" ||
    event.type === "reasoning_summary_delta"
  ) {
    return isEffectivelyEmptyTextEntry(entry) ? "neutral" : "thinking";
  }

  if (event.type === "agent_message_delta") {
    return isEffectivelyEmptyTextEntry(entry) ? "neutral" : "hard_boundary";
  }

  if (
    event.type === "turn_plan_updated" ||
    event.type === "user_input_submitted" ||
    event.type === "approval_request"
  ) {
    return "hard_boundary";
  }

  if (event.type === "error") {
    return isWillRetryErrorEvent(event) ? "neutral" : "hard_boundary";
  }

  if (
    event.type === "token_usage_updated" ||
    event.type === "thread_status_changed" ||
    event.type === "executor_context_compacted" ||
    event.type === "turn_diff_updated" ||
    event.type === "plan_delta"
  ) {
    return "neutral";
  }

  if (event.type === "platform") {
    const boundary = getPlatformEventPolicy(event, options).feedBoundary;
    if (boundary === "hard") return "hard_boundary";
    if (boundary === "soft") return "hard_boundary";
    return "neutral";
  }

  return "hard_boundary";
}

function createToolGroup(entry: SessionDisplayEntry): AggregatedEntryGroup {
  return {
    type: "aggregated_group",
    aggregationType: "tool_burst",
    entries: [entry],
    id: entry.id,
    groupKey: `tool-${entry.id}`,
  };
}

function appendToolEntry(
  group: AggregatedEntryGroup | null,
  entry: SessionDisplayEntry,
): AggregatedEntryGroup {
  if (group) {
    group.entries.push(entry);
    return group;
  }
  return createToolGroup(entry);
}

function pushToolGroup(
  result: SessionDisplayItem[],
  group: AggregatedEntryGroup | null,
): null {
  if (!group) return null;
  if (group.entries.length === 1) {
    const only = group.entries[0];
    if (only) result.push(only);
    return null;
  }
  result.push(group);
  return null;
}

// ── side group：context_frame 内部聚合，并作为运行期上下文硬边界截断工具 burst ──
//
// 注：reasoning_text_delta/summary 同 itemId 已在 useSessionStream 层累积为单条
// entry，因此 thinking 没有"连续多条"场景，无需聚合。

function isCtxSideGroup(
  group: AggregatedContextFrameGroup | null,
): group is AggregatedContextFrameGroup {
  return group?.type === "aggregated_context_frames";
}

function createCtxSideGroup(entry: SessionDisplayEntry): AggregatedContextFrameGroup {
  return {
    type: "aggregated_context_frames",
    entries: [entry],
    id: entry.id,
    groupKey: `context-frame-${entry.id}`,
  };
}

function isThinkingEvent(event: BackboneEvent): boolean {
  return event.type === "reasoning_text_delta" || event.type === "reasoning_summary_delta";
}

function isAgentMessageEvent(event: BackboneEvent): boolean {
  return event.type === "agent_message_delta";
}

function isUserInputItem(item: SessionDisplayItem): boolean {
  return "event" in item && (item as SessionDisplayEntry).event.type === "user_input_submitted";
}

function displayItemSeq(item: SessionDisplayItem): number {
  if ("eventSeq" in item && typeof item.eventSeq === "number") {
    return item.eventSeq;
  }
  if ("entries" in item) {
    return item.entries[0]?.eventSeq ?? Number.MAX_SAFE_INTEGER;
  }
  return Number.MAX_SAFE_INTEGER;
}

function displayItemTurnId(item: SessionDisplayItem): string | undefined {
  if ("turnId" in item && typeof item.turnId === "string") {
    return item.turnId;
  }
  if ("entries" in item) {
    return item.entries[0]?.turnId;
  }
  return undefined;
}

interface TurnThinkingState {
  turnId: string;
  waitingSeq?: number;
  firstSeq: number;
  insertionIndex?: number;
  hasAgentMessage: boolean;
  entries: SessionDisplayEntry[];
}

function ensureTurnThinkingState(
  map: Map<string, TurnThinkingState>,
  turnId: string,
  firstSeq: number,
): TurnThinkingState {
  const existing = map.get(turnId);
  if (existing) {
    if (firstSeq < existing.firstSeq) existing.firstSeq = firstSeq;
    return existing;
  }
  const created: TurnThinkingState = {
    turnId,
    firstSeq,
    hasAgentMessage: false,
    entries: [],
  };
  map.set(turnId, created);
  return created;
}

function markThinkingInsertionIndex(
  state: TurnThinkingState,
  insertionIndex: number,
): void {
  if (state.insertionIndex == null || insertionIndex < state.insertionIndex) {
    state.insertionIndex = insertionIndex;
  }
}

function extractProviderWaitingSeqs(rawEvents: SessionEventEnvelope[]): Map<string, number> {
  const result = new Map<string, number>();
  for (const event of [...rawEvents].sort((a, b) => a.event_seq - b.event_seq)) {
    const status = extractProviderAttemptStatus(event);
    if (!status?.turnId) {
      continue;
    }
    if (status.phase === "connected_waiting_first_delta") {
      result.set(status.turnId, event.event_seq);
    } else {
      result.delete(status.turnId);
    }
  }
  return result;
}

function createThinkingGroup(state: TurnThinkingState): AggregatedThinkingGroup | null {
  const hasThinkingText = state.entries.length > 0;
  const isStreamingThinking = state.waitingSeq != null && !state.hasAgentMessage;
  if (!hasThinkingText && !isStreamingThinking) {
    return null;
  }

  const eventSeq = state.waitingSeq ?? state.entries[0]?.eventSeq ?? state.firstSeq;
  return {
    type: "aggregated_thinking",
    entries: state.entries,
    id: `thinking:${state.turnId}:${eventSeq}`,
    groupKey: `thinking:${state.turnId}`,
    turnId: state.turnId,
    eventSeq,
    isStreamingThinking,
  };
}

function mergeThinkingIntoDisplayItems(
  displayItems: SessionDisplayItem[],
  rawEvents: SessionEventEnvelope[],
): SessionDisplayItem[] {
  const waitingSeqs = extractProviderWaitingSeqs(rawEvents);
  if (displayItems.length === 0 && waitingSeqs.size === 0) {
    return displayItems;
  }

  const thinkingStates = new Map<string, TurnThinkingState>();
  for (const [turnId, waitingSeq] of waitingSeqs.entries()) {
    const state = ensureTurnThinkingState(thinkingStates, turnId, waitingSeq);
    state.waitingSeq = waitingSeq;
  }

  const nonThinkingItems: SessionDisplayItem[] = [];
  for (const item of displayItems) {
    const turnId = displayItemTurnId(item);
    const eventSeq = displayItemSeq(item);

    if ("event" in item && isThinkingEvent(item.event)) {
      if (turnId) {
        const state = ensureTurnThinkingState(thinkingStates, turnId, eventSeq);
        markThinkingInsertionIndex(state, nonThinkingItems.length);
        state.entries.push(item);
      }
      continue;
    }

    if ("type" in item && (item as AggregatedThinkingGroup).type === "aggregated_thinking") {
      const group = item as AggregatedThinkingGroup;
      const groupTurnId = group.turnId ?? group.entries[0]?.turnId;
      if (groupTurnId) {
        const state = ensureTurnThinkingState(thinkingStates, groupTurnId, group.eventSeq);
        markThinkingInsertionIndex(state, nonThinkingItems.length);
        state.entries.push(...group.entries);
      }
      continue;
    }

    if (turnId) {
      const existingState = thinkingStates.get(turnId);
      if (existingState) {
        markThinkingInsertionIndex(existingState, nonThinkingItems.length);
      }
    }

    if ("event" in item && isAgentMessageEvent(item.event) && turnId) {
      ensureTurnThinkingState(thinkingStates, turnId, eventSeq).hasAgentMessage = true;
    }
    nonThinkingItems.push(item);
  }

  const thinkingGroups = new Map<number, AggregatedThinkingGroup[]>();
  for (const state of thinkingStates.values()) {
    const group = createThinkingGroup(state);
    if (!group) continue;
    const insertionIndex = state.insertionIndex ?? nonThinkingItems.length;
    const groups = thinkingGroups.get(insertionIndex);
    if (groups) {
      groups.push(group);
    } else {
      thinkingGroups.set(insertionIndex, [group]);
    }
  }

  const merged: SessionDisplayItem[] = [];
  for (let index = 0; index <= nonThinkingItems.length; index += 1) {
    const groups = thinkingGroups.get(index);
    if (groups) {
      merged.push(...groups);
    }
    const item = nonThinkingItems[index];
    if (item) {
      merged.push(item);
    }
  }
  return merged;
}

function pushCtxSideGroup(
  result: SessionDisplayItem[],
  group: AggregatedContextFrameGroup | null,
): null {
  if (!group) return null;
  if (group.entries.length === 1) {
    const only = group.entries[0];
    if (only) result.push(only);
    return null;
  }
  result.push(group);
  return null;
}

/**
 * 聚合 entries → display items。
 *
 * **关键约定：合并只覆盖同类内部，绝不跨类。**
 * - tool_like：连续工具调用合并为 tool burst
 * - context_frame：连续 CTX 合并为 CTX group，并作为 hard boundary flush tool group
 * - 其他（agent message / reasoning / approval / error / 可渲染 hook）：
 *   hard boundary，自身单 entry，flush tool group
 * - neutral：完全透明
 *
 * CTX 表达 Agent 可见上下文已经改变，工具 burst 不能跨过 CTX 合并。
 *
 * Reasoning 不参与聚合 —— 同 itemId 已在 useSessionStream 层累积成一条，
 * 不会出现"连续多条 thinking entry"的场景。
 */
function aggregateEntries(
  entries: SessionDisplayEntry[],
  options: AggregateEntriesOptions = {},
): SessionDisplayItem[] {
  const result: SessionDisplayItem[] = [];
  let activeToolGroup: AggregatedEntryGroup | null = null;
  let activeCtxGroup: AggregatedContextFrameGroup | null = null;

  const flushToolGroup = () => {
    activeToolGroup = pushToolGroup(result, activeToolGroup);
  };
  const flushCtxGroup = () => {
    activeCtxGroup = pushCtxSideGroup(result, activeCtxGroup);
  };

  const joinCtxGroup = (entry: SessionDisplayEntry) => {
    const currentCtxGroup = activeCtxGroup;
    if (isCtxSideGroup(currentCtxGroup)) {
      currentCtxGroup.entries.push(entry);
    } else {
      flushCtxGroup();
      activeCtxGroup = createCtxSideGroup(entry);
    }
  };

  for (const entry of entries) {
    const cls = classifyEntry(entry, options);

    switch (cls) {
      case "tool_like": {
        flushCtxGroup();
        activeToolGroup = appendToolEntry(activeToolGroup, entry);
        break;
      }

      case "tool_single": {
        flushToolGroup();
        flushCtxGroup();
        result.push(entry);
        break;
      }

      case "thinking": {
        flushToolGroup();
        flushCtxGroup();
        result.push(entry);
        break;
      }

      case "hard_boundary": {
        flushToolGroup();
        if (isContextFrameEvent(entry.event)) {
          joinCtxGroup(entry);
        } else {
          flushCtxGroup();
          result.push(entry);
        }
        break;
      }

      case "neutral":
      default: {
        break;
      }
    }
  }

  flushToolGroup();
  flushCtxGroup();

  return result;
}

export { aggregateEntries, mergeThinkingIntoDisplayItems };

// ── Turn 分段 ──

export type TurnStatus = "active" | "completed" | "failed" | "interrupted";

export type TurnActivityKind =
  | "connecting"
  | "reconnecting"
  | "retry_exhausted";

export interface TurnActivityStatus {
  kind: TurnActivityKind;
  label: string;
  phase?: string;
  attempt?: number;
  maxAttempts?: number;
}

export interface TurnSegment {
  turnId: string | null;
  status: TurnStatus;
  durationMs?: number;
  activity?: TurnActivityStatus;
  items: SessionDisplayItem[];
  /** 最后一条 agent message（轮次折叠时只显示这个） */
  finalOutput: SessionDisplayItem | null;
}

function extractTurnId(item: SessionDisplayItem): string | undefined {
  const thinkingTurnId = displayItemTurnId(item);
  if (thinkingTurnId) return thinkingTurnId;
  if ("type" in item && (item as AggregatedEntryGroup).type === "aggregated_group") {
    const group = item as AggregatedEntryGroup;
    return group.entries[0]?.turnId;
  }
  if ("turnId" in item) {
    return (item as SessionDisplayEntry).turnId;
  }
  return undefined;
}

function isAgentMessageItem(item: SessionDisplayItem): boolean {
  if (!("event" in item)) return false;
  return (item as SessionDisplayEntry).event.type === "agent_message_delta";
}

interface TurnMeta {
  status: TurnStatus;
  firstSeq: number;
  durationMs?: number;
  activity?: TurnActivityStatus;
}

interface ProviderAttemptStatusPayload {
  turnId?: string;
  phase: string;
  attempt?: number;
  maxAttempts?: number;
  willRetry?: boolean;
  message?: string;
}

const PROVIDER_STATUS_META_KEYS = new Set([
  "provider_attempt_status",
  "provider_retry",
  "provider_status",
]);

function readStringField(record: Record<string, unknown>, key: string): string | undefined {
  const value = record[key];
  return typeof value === "string" && value.trim().length > 0 ? value : undefined;
}

function readNumberField(record: Record<string, unknown>, key: string): number | undefined {
  const value = record[key];
  if (typeof value === "number" && Number.isFinite(value)) {
    return value;
  }
  if (
    typeof value === "bigint" &&
    value >= BigInt(Number.MIN_SAFE_INTEGER) &&
    value <= BigInt(Number.MAX_SAFE_INTEGER)
  ) {
    return Number(value);
  }
  return undefined;
}

function readBooleanField(record: Record<string, unknown>, key: string): boolean | undefined {
  const value = record[key];
  return typeof value === "boolean" ? value : undefined;
}

function eventTurnId(event: SessionEventEnvelope): string | undefined {
  return event.turn_id ?? event.notification.trace.turnId ?? undefined;
}

function ensureTurnMeta(
  map: Map<string, TurnMeta>,
  turnId: string,
  firstSeq: number,
): TurnMeta {
  const existing = map.get(turnId);
  if (existing) {
    if (firstSeq < existing.firstSeq) {
      existing.firstSeq = firstSeq;
    }
    return existing;
  }
  const created: TurnMeta = { status: "active", firstSeq };
  map.set(turnId, created);
  return created;
}

function updateTurnMeta(
  map: Map<string, TurnMeta>,
  turnId: string,
  firstSeq: number,
  patch: Partial<Omit<TurnMeta, "firstSeq">>,
): void {
  const meta = ensureTurnMeta(map, turnId, firstSeq);
  if (patch.status) meta.status = patch.status;
  if (patch.durationMs !== undefined) meta.durationMs = patch.durationMs;
  if (patch.activity !== undefined) meta.activity = patch.activity;
}

function normalizeTurnStatus(status: string): TurnStatus {
  if (status === "completed") return "completed";
  if (status === "failed") return "failed";
  if (status === "interrupted") return "interrupted";
  return "active";
}

function extractTurnTerminalMeta(event: SessionEventEnvelope): {
  turnId: string;
  status: TurnStatus;
  durationMs?: number;
} | null {
  const bbEvent = event.notification.event;
  if (bbEvent.type === "turn_completed") {
    const turn = bbEvent.payload.turn;
    return {
      turnId: turn.id,
      status: normalizeTurnStatus(turn.status),
      durationMs: turn.durationMs ?? undefined,
    };
  }

  if (
    bbEvent.type !== "platform" ||
    bbEvent.payload.kind !== "session_meta_update" ||
    bbEvent.payload.data.key !== "turn_terminal" ||
    !isRecord(bbEvent.payload.data.value)
  ) {
    return null;
  }

  const value = bbEvent.payload.data.value;
  const terminalType = readStringField(value, "terminal_type");
  const turnId = readStringField(value, "turn_id") ?? eventTurnId(event);
  if (!terminalType || !turnId) {
    return null;
  }
  const status: TurnStatus =
    terminalType === "turn_completed" ? "completed"
    : terminalType === "turn_interrupted" ? "interrupted"
    : "failed";
  return {
    turnId,
    status,
    durationMs: readNumberField(value, "duration_ms"),
  };
}

function extractProviderAttemptStatus(event: SessionEventEnvelope): ProviderAttemptStatusPayload | null {
  const bbEvent = event.notification.event;
  if (bbEvent.type === "error" && bbEvent.payload.willRetry) {
    return {
      turnId: bbEvent.payload.turnId || eventTurnId(event),
      phase: "retrying",
      willRetry: true,
      message: bbEvent.payload.error.message,
    };
  }

  if (bbEvent.type !== "platform" || !isRecord(bbEvent.payload)) {
    return null;
  }

  const platform: Record<string, unknown> = bbEvent.payload;
  const kind = readStringField(platform, "kind");
  let data: Record<string, unknown> | null = null;

  if (kind === "provider_attempt_status") {
    data = isRecord(platform.data) ? platform.data : null;
  } else if (kind === "session_meta_update" && isRecord(platform.data)) {
    const metaData = platform.data;
    const key = readStringField(metaData, "key");
    if (key && PROVIDER_STATUS_META_KEYS.has(key) && isRecord(metaData.value)) {
      data = metaData.value;
    }
  }

  if (!data) {
    return null;
  }

  const phase = readStringField(data, "phase");
  if (!phase) {
    return null;
  }

  return {
    turnId: readStringField(data, "turn_id") ?? eventTurnId(event),
    phase,
    attempt: readNumberField(data, "attempt"),
    maxAttempts: readNumberField(data, "max_attempts"),
    willRetry: readBooleanField(data, "will_retry"),
    message: readStringField(data, "message"),
  };
}

export function segmentByTurn(
  displayItems: SessionDisplayItem[],
  rawEvents: SessionEventEnvelope[],
): TurnSegment[] {
  const turnMeta = new Map<string, TurnMeta>();

  for (const event of rawEvents) {
    const bbEvent = event.notification.event;

    if (bbEvent.type === "turn_started") {
      updateTurnMeta(turnMeta, bbEvent.payload.turn.id, event.event_seq, { status: "active" });
    }

    const terminal = extractTurnTerminalMeta(event);
    if (terminal) {
      updateTurnMeta(turnMeta, terminal.turnId, event.event_seq, {
        status: terminal.status,
        durationMs: terminal.durationMs,
      });
    }

  }

  if (displayItems.length === 0) {
    return [...turnMeta.entries()]
      .filter(([, meta]) => meta.activity)
      .sort((a, b) => a[1].firstSeq - b[1].firstSeq)
      .map(([turnId, meta]) => ({
        turnId,
        status: meta.status,
        durationMs: meta.durationMs,
        activity: meta.activity,
        items: [],
        finalOutput: null,
      }));
  }

  const segments: TurnSegment[] = [];
  const seenTurnIds = new Set<string>();
  let currentTurnId: string | null = null;
  let currentItems: SessionDisplayItem[] = [];

  const flush = () => {
    if (currentItems.length === 0) return;
    const meta = currentTurnId ? turnMeta.get(currentTurnId) : undefined;
    if (currentTurnId) {
      seenTurnIds.add(currentTurnId);
    }
    let finalOutput: SessionDisplayItem | null = null;
    for (let i = currentItems.length - 1; i >= 0; i -= 1) {
      if (isAgentMessageItem(currentItems[i]!)) {
        finalOutput = currentItems[i]!;
        break;
      }
    }
    segments.push({
      turnId: currentTurnId,
      status: meta?.status ?? "active",
      durationMs: meta?.durationMs,
      activity: meta?.activity,
      items: currentItems,
      finalOutput,
    });
    currentItems = [];
  };

  const pushUserItem = (item: SessionDisplayItem) => {
    segments.push({
      turnId: null,
      status: "active",
      durationMs: undefined,
      activity: undefined,
      items: [item],
      finalOutput: null,
    });
  };

  for (const item of displayItems) {
    if (isUserInputItem(item)) {
      flush();
      currentTurnId = null;
      pushUserItem(item);
      continue;
    }

    const turnId = extractTurnId(item) ?? null;
    if (turnId !== currentTurnId) {
      flush();
      currentTurnId = turnId;
    }
    currentItems.push(item);
  }
  flush();

  const missingStatusSegments = [...turnMeta.entries()]
    .filter(([turnId]) => !seenTurnIds.has(turnId))
    .filter(([, meta]) => meta.activity)
    .sort((a, b) => a[1].firstSeq - b[1].firstSeq)
    .map(([turnId, meta]): TurnSegment => ({
      turnId,
      status: meta.status,
      durationMs: meta.durationMs,
      activity: meta.activity,
      items: [],
      finalOutput: null,
    }));

  segments.push(...missingStatusSegments);

  return segments;
}

export function useSessionFeed(options: UseSessionFeedOptions): UseSessionFeedResult {
  const { sessionId, endpoint, enableAggregation = true, enabled } = options;
  const { prefs } = useDebugPrefs();

  const {
    entries,
    rawEvents,
    isConnected,
    isLoading,
    isReceiving,
    error,
    tokenUsage,
    reconnect,
    close,
    sendCancel,
  } = useSessionStream({
    sessionId,
    endpoint,
    enabled,
  });

  const displayItems = useMemo(() => {
    const baseDisplayItems = enableAggregation
      ? aggregateEntries(entries, { includeVerboseEvents: prefs.hookVerbose })
      : (entries as SessionDisplayItem[]);
    return enableAggregation
      ? mergeThinkingIntoDisplayItems(baseDisplayItems, rawEvents)
      : baseDisplayItems;
  }, [entries, rawEvents, enableAggregation, prefs.hookVerbose]);

  const turnSegments = useMemo(
    () => segmentByTurn(displayItems, rawEvents),
    [displayItems, rawEvents],
  );

  const streamingEntryId = useMemo(() => {
    if (!isReceiving || entries.length === 0) return null;
    const last = entries[entries.length - 1]!;
    if (last.event.type === "agent_message_delta") return last.id;
    return null;
  }, [isReceiving, entries]);

  return {
    displayItems,
    turnSegments,
    rawEntries: entries,
    rawEvents,
    isConnected,
    isLoading,
    isReceiving,
    error,
    reconnect,
    close,
    sendCancel,
    streamingEntryId,
    tokenUsage,
  };
}

export default useSessionFeed;
