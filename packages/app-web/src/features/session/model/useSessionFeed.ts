/**
 * 会话管理 Hook
 *
 * 整合流管理和条目聚合逻辑。
 * 暴露 displayItems（聚合后）、rawEntries、tokenUsage 等供 UI 使用。
 */

import { useMemo, useRef } from "react";
import { useSessionStream } from "./useSessionStream";
import type { BackboneEvent, AgentDashThreadItem } from "../../../generated/backbone-protocol";
import {
  isAggregatedGroup as isAggregatedGroupItem,
  isAggregatedContextFrameGroup as isAggregatedContextFrameGroupItem,
  isAggregatedThinkingGroup as isAggregatedThinkingGroupItem,
} from "./types";
import { isRenderablePlatformEvent } from "./systemEventVisibility";
import type {
  AggregatedContextFrameGroup,
  SessionDisplayEntry,
  SessionDisplayItem,
  AggregatedEntryGroup,
  AggregatedThinkingGroup,
  SessionEventEnvelope,
  ToolAggregationType,
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

function getToolAggregationType(event: BackboneEvent): ToolAggregationType | null {
  const item = extractThreadItem(event);
  if (!item) return null;

  switch (item.type) {
    case "commandExecution":
    case "fileChange":
    case "mcpToolCall":
    case "dynamicToolCall":
    case "collabAgentToolCall":
    case "webSearch":
    case "imageView":
    case "imageGeneration":
    case "fsRead":
    case "fsGrep":
    case "fsGlob":
      return "tool_burst";
    default:
      return null;
  }
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
  | "hard_boundary"
  | "soft_boundary"
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

function isUserMessageChunk(event: BackboneEvent): boolean {
  return (
    event.type === "platform" &&
    event.payload.kind === "session_meta_update" &&
    event.payload.data.key === "user_message_chunk"
  );
}

function classifyEntry(entry: SessionDisplayEntry): EntryClassification {
  const event = entry.event;
  if (getToolAggregationType(event) !== null) {
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
    // user_message_chunk → 真正的"用户/agent 可见产出"，硬边界
    if (isUserMessageChunk(event)) return "hard_boundary";
    // context_frame → 侧轨身份/能力切换，不应打散 tool burst
    if (isContextFrameEvent(event)) return "soft_boundary";
    // 其他可渲染系统/任务事件 → 硬边界（hook trace、companion、capability change 等）
    if (isRenderablePlatformEvent(event)) return "hard_boundary";
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

// ── side group：context_frame 内部聚合，但**不**与工具组合并 ──
//
// 注：reasoning_text_delta/summary 同 itemId 已在 useSessionStream 层累积为单条
// entry，因此 thinking 没有"连续多条"场景，无需聚合。

function isCtxSideGroup(group: AggregatedContextFrameGroup | null): boolean {
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
 * - context_frame：连续 CTX 合并为 CTX group（soft boundary，**不** flush tool）
 * - 其他（agent message / reasoning / approval / error / 可渲染 hook）：
 *   hard boundary，自身单 entry，flush tool group
 * - neutral：完全透明
 *
 * CTX 是 soft boundary 的关键：它出现在工具序列中间时，不会把工具组打散，
 * 仅独立成自己的 CTX group 与工具组并存。
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
    if (isCtxSideGroup(activeCtxGroup)) {
      activeCtxGroup!.entries.push(entry);
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

      case "hard_boundary": {
        flushToolGroup();
        flushCtxGroup();
        result.push(entry);
        break;
      }

      case "soft_boundary": {
        // CTX：不 flush tool group，进 CTX side group 内部聚合
        if (isContextFrameEvent(entry.event)) {
          joinCtxGroup(entry);
        } else {
          // 防御：当前 soft_boundary 只覆盖 context_frame
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

function entryShallowEqual(a: SessionDisplayEntry, b: SessionDisplayEntry): boolean {
  return (
    a.id === b.id &&
    a.eventSeq === b.eventSeq &&
    a.event === b.event &&
    a.isPendingApproval === b.isPendingApproval
  );
}

function isAggregatedGroupEqual(a: SessionDisplayItem, b: SessionDisplayItem): boolean {
  if (a === b) return true;

  const aIsGroup = isAggregatedGroupItem(a);
  const bIsGroup = isAggregatedGroupItem(b);
  if (aIsGroup !== bIsGroup) return false;

  const aIsThink = isAggregatedThinkingGroupItem(a);
  const bIsThink = isAggregatedThinkingGroupItem(b);
  if (aIsThink !== bIsThink) return false;

  const aIsContextFrame = isAggregatedContextFrameGroupItem(a);
  const bIsContextFrame = isAggregatedContextFrameGroupItem(b);
  if (aIsContextFrame !== bIsContextFrame) return false;

  if (aIsGroup && bIsGroup) {
    const ga = a as AggregatedEntryGroup;
    const gb = b as AggregatedEntryGroup;
    if (ga.groupKey !== gb.groupKey) return false;
    if (ga.aggregationType !== gb.aggregationType) return false;
    if (ga.entries.length !== gb.entries.length) return false;
    for (let i = 0; i < ga.entries.length; i += 1) {
      if (!entryShallowEqual(ga.entries[i]!, gb.entries[i]!)) return false;
    }
    return true;
  }

  if (aIsThink && bIsThink) {
    const ta = a as AggregatedThinkingGroup;
    const tb = b as AggregatedThinkingGroup;
    if (ta.groupKey !== tb.groupKey) return false;
    if (ta.entries.length !== tb.entries.length) return false;
    for (let i = 0; i < ta.entries.length; i += 1) {
      if (!entryShallowEqual(ta.entries[i]!, tb.entries[i]!)) return false;
    }
    return true;
  }

  if (aIsContextFrame && bIsContextFrame) {
    const ca = a as AggregatedContextFrameGroup;
    const cb = b as AggregatedContextFrameGroup;
    if (ca.groupKey !== cb.groupKey) return false;
    if (ca.entries.length !== cb.entries.length) return false;
    for (let i = 0; i < ca.entries.length; i += 1) {
      if (!entryShallowEqual(ca.entries[i]!, cb.entries[i]!)) return false;
    }
    return true;
  }

  return entryShallowEqual(a as SessionDisplayEntry, b as SessionDisplayEntry);
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

  const prevDisplayItemsRef = useRef<SessionDisplayItem[]>([]);

  /* eslint-disable react-hooks/refs */
  const displayItems = useMemo(() => {
    const next: SessionDisplayItem[] = enableAggregation
      ? aggregateEntries(entries)
      : (entries as SessionDisplayItem[]);

    const prev = prevDisplayItemsRef.current;
    if (prev.length === next.length) {
      let allEqual = true;
      const stabilized: SessionDisplayItem[] = new Array(next.length);
      for (let i = 0; i < next.length; i += 1) {
        const a = prev[i]!;
        const b = next[i]!;
        if (isAggregatedGroupEqual(a, b)) {
          stabilized[i] = a;
        } else {
          stabilized[i] = b;
          allEqual = false;
        }
      }
      if (allEqual && prev.every((p, i) => p === stabilized[i])) {
        return prev;
      }
      prevDisplayItemsRef.current = stabilized;
      return stabilized;
    }
    prevDisplayItemsRef.current = next;
    return next;
  }, [entries, enableAggregation]);
  /* eslint-enable react-hooks/refs */

  const streamingEntryId = useMemo(() => {
    if (!isReceiving || entries.length === 0) return null;
    const last = entries[entries.length - 1]!;
    if (last.event.type === "agent_message_delta") return last.id;
    return null;
  }, [isReceiving, entries]);

  /* eslint-disable react-hooks/refs */
  return {
    displayItems,
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
