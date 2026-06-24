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
  if (event.type === "item_started" || event.type === "item_completed") {
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
  | "active_tool"
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

function isToolEntryTerminal(entry: SessionDisplayEntry): boolean {
  const item = extractThreadItem(entry.event);
  if (!item) return true;
  if (!("status" in item)) return true;
  return item.status !== "inProgress";
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
    if (hasBoundedOutputEntry(entry)) return "active_tool";
    if (!isToolEntryTerminal(entry)) return "active_tool";
    return "tool_like";
  }

  if (event.type === "turn_started" || event.type === "turn_completed") {
    return "neutral";
  }

  if (
    event.type === "agent_message_delta" ||
    event.type === "reasoning_text_delta" ||
    event.type === "reasoning_summary_delta"
  ) {
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

      case "active_tool": {
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

export { aggregateEntries };

// ── Turn 分段 ──

export type TurnStatus = "active" | "completed" | "failed" | "interrupted";

export type TurnActivityKind =
  | "connecting"
  | "thinking"
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

function formatAttempt(status: ProviderAttemptStatusPayload): string | null {
  if (status.attempt == null && status.maxAttempts == null) return null;
  if (status.attempt != null && status.maxAttempts != null) {
    return `${status.attempt}/${status.maxAttempts}`;
  }
  if (status.attempt != null) return `${status.attempt}`;
  return `/${status.maxAttempts}`;
}

function providerStatusToActivity(status: ProviderAttemptStatusPayload): TurnActivityStatus | null {
  const attemptText = formatAttempt(status);
  switch (status.phase) {
    case "connecting":
      return {
        kind: "connecting",
        label: "连接模型服务",
        phase: status.phase,
        attempt: status.attempt,
        maxAttempts: status.maxAttempts,
      };
    case "connected_waiting_first_delta":
      return {
        kind: "thinking",
        label: "正在思考",
        phase: status.phase,
        attempt: status.attempt,
        maxAttempts: status.maxAttempts,
      };
    case "retry_scheduled":
    case "retrying":
      return {
        kind: "reconnecting",
        label: status.message ?? (attemptText ? `Reconnecting... ${attemptText}` : "正在重连模型服务"),
        phase: status.phase,
        attempt: status.attempt,
        maxAttempts: status.maxAttempts,
      };
    case "failed":
    case "retry_exhausted":
    case "exhausted":
      if (status.willRetry === false || status.phase !== "failed") {
        return {
          kind: "retry_exhausted",
          label: status.message ?? "重试已耗尽",
          phase: status.phase,
          attempt: status.attempt,
          maxAttempts: status.maxAttempts,
        };
      }
      return null;
    default:
      return null;
  }
}

function isVisibleTurnOutput(event: SessionEventEnvelope): boolean {
  const bbEvent = event.notification.event;
  if (
    bbEvent.type === "agent_message_delta" ||
    bbEvent.type === "reasoning_text_delta" ||
    bbEvent.type === "reasoning_summary_delta"
  ) {
    return bbEvent.payload.delta.trim().length > 0;
  }
  return bbEvent.type === "item_started" ||
    bbEvent.type === "item_completed" ||
    bbEvent.type === "command_output_delta" ||
    bbEvent.type === "file_change_delta" ||
    bbEvent.type === "mcp_tool_call_progress";
}

export function segmentByTurn(
  displayItems: SessionDisplayItem[],
  rawEvents: SessionEventEnvelope[],
): TurnSegment[] {
  const turnMeta = new Map<string, TurnMeta>();
  const visibleOutputTurnIds = new Set<string>();

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

    const providerStatus = extractProviderAttemptStatus(event);
    if (providerStatus?.turnId) {
      const activity = providerStatusToActivity(providerStatus);
      if (activity) {
        updateTurnMeta(turnMeta, providerStatus.turnId, event.event_seq, { activity });
      }
    }

    const turnId = eventTurnId(event);
    if (turnId && isVisibleTurnOutput(event)) {
      visibleOutputTurnIds.add(turnId);
    }
  }

  for (const [turnId, meta] of turnMeta.entries()) {
    if (meta.status === "active" && !meta.activity && !visibleOutputTurnIds.has(turnId)) {
      meta.activity = { kind: "thinking", label: "正在思考", phase: "in_progress" };
    }
  }

  if (displayItems.length === 0) {
    return [...turnMeta.entries()]
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

  for (const item of displayItems) {
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
    return enableAggregation
      ? aggregateEntries(entries, { includeVerboseEvents: prefs.hookVerbose })
      : (entries as SessionDisplayItem[]);
  }, [entries, enableAggregation, prefs.hookVerbose]);

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
