/**
 * 会话管理 Hook
 *
 * 整合流管理和条目聚合逻辑。
 * 暴露 displayItems（聚合后）、rawEntries、tokenUsage 等供 UI 使用。
 */

import { useMemo } from "react";
import { useSessionStream } from "./useSessionStream";
import type { BackboneEvent, AgentDashThreadItem } from "../../../generated/backbone-protocol";
import { getPlatformEventPolicy } from "./systemEventPolicy";
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

function isContextFrameEvent(event: BackboneEvent): boolean {
  return (
    event.type === "platform" &&
    event.payload.kind === "session_meta_update" &&
    event.payload.data.key === "context_frame"
  );
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

function classifyEntry(entry: SessionDisplayEntry): EntryClassification {
  const event = entry.event;
  if (isToolBurstEvent(event)) {
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
    event.type === "approval_request" ||
    event.type === "error"
  ) {
    return "hard_boundary";
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
    const boundary = getPlatformEventPolicy(event).feedBoundary;
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
function aggregateEntries(entries: SessionDisplayEntry[]): SessionDisplayItem[] {
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
    const cls = classifyEntry(entry);

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

export interface TurnSegment {
  turnId: string | null;
  status: TurnStatus;
  durationMs?: number;
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

export function segmentByTurn(
  displayItems: SessionDisplayItem[],
  rawEvents: SessionEventEnvelope[],
): TurnSegment[] {
  const turnMeta = new Map<string, { status: TurnStatus; durationMs?: number }>();
  for (const event of rawEvents) {
    const bbEvent = event.notification.event;
    if (bbEvent.type === "turn_completed") {
      const turn = bbEvent.payload.turn;
      const status: TurnStatus =
        turn.status === "completed" ? "completed"
        : turn.status === "failed" ? "failed"
        : turn.status === "interrupted" ? "interrupted"
        : "active";
      turnMeta.set(turn.id, {
        status,
        durationMs: turn.durationMs ?? undefined,
      });
    }
  }

  if (displayItems.length === 0) return [];

  const segments: TurnSegment[] = [];
  let currentTurnId: string | null = null;
  let currentItems: SessionDisplayItem[] = [];

  const flush = () => {
    if (currentItems.length === 0) return;
    const meta = currentTurnId ? turnMeta.get(currentTurnId) : undefined;
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

  return segments;
}

export function useSessionFeed(options: UseSessionFeedOptions): UseSessionFeedResult {
  const { sessionId, endpoint, enableAggregation = true, enabled } = options;

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
      ? aggregateEntries(entries)
      : (entries as SessionDisplayItem[]);
  }, [entries, enableAggregation]);

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
