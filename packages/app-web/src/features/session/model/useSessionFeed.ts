/**
 * 会话管理 Hook
 *
 * 整合流管理和条目聚合逻辑。
 * 暴露 displayItems（聚合后）、rawEntries、tokenUsage 等供 UI 使用。
 */

import { useMemo, useRef } from "react";
import { useSessionStream } from "./useSessionStream";
import type { BackboneEvent, ThreadItem } from "../../../generated/backbone-protocol";
import {
  isAggregatedGroup as isAggregatedGroupItem,
  isAggregatedContextFrameGroup as isAggregatedContextFrameGroupItem,
  isAggregatedThinkingGroup as isAggregatedThinkingGroupItem,
} from "./types";
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

function extractThreadItem(event: BackboneEvent): ThreadItem | null {
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
    case "webSearch":
      return "turn_fold";
    default:
      return null;
  }
}

function isThinkingEvent(event: BackboneEvent): boolean {
  return event.type === "reasoning_text_delta" || event.type === "reasoning_summary_delta";
}

function isContextFrameEvent(event: BackboneEvent): boolean {
  return (
    event.type === "platform" &&
    event.payload.kind === "session_meta_update" &&
    event.payload.data.key === "context_frame"
  );
}

function isNonAggregatableEvent(event: BackboneEvent): boolean {
  return (
    event.type === "platform" ||
    event.type === "token_usage_updated" ||
    event.type === "thread_status_changed" ||
    event.type === "error" ||
    event.type === "approval_request"
  );
}

type EntryClassification =
  | "turn_boundary"
  | "message"
  | "tool_like"
  | "thinking"
  | "context_frame"
  | "non_agg";

function classifyEntry(entry: SessionDisplayEntry): EntryClassification {
  const event = entry.event;
  if (event.type === "turn_started" || event.type === "turn_completed") {
    return "turn_boundary";
  }
  if (event.type === "agent_message_delta") {
    return "message";
  }
  if (isThinkingEvent(event)) {
    return "thinking";
  }
  if (isContextFrameEvent(event)) {
    return "context_frame";
  }
  if (getToolAggregationType(event) !== null) {
    return "tool_like";
  }
  if (isNonAggregatableEvent(event)) {
    return "non_agg";
  }
  return "non_agg";
}

function isEffectivelyEmptyMessage(entry: SessionDisplayEntry): boolean {
  if (entry.event.type !== "agent_message_delta") return false;
  const text = entry.accumulatedText ?? entry.event.payload.delta ?? "";
  return text.trim().length === 0;
}

function aggregateEntries(entries: SessionDisplayEntry[]): SessionDisplayItem[] {
  const result: SessionDisplayItem[] = [];
  let currentUnit: AggregatedEntryGroup | null = null;
  let currentThinkingGroup: AggregatedThinkingGroup | null = null;
  let currentContextFrameGroup: AggregatedContextFrameGroup | null = null;

  const flushUnit = () => {
    if (currentUnit) {
      result.push(currentUnit);
      currentUnit = null;
    }
  };

  const flushThinking = () => {
    if (currentThinkingGroup) {
      result.push(currentThinkingGroup);
      currentThinkingGroup = null;
    }
  };

  const flushContextFrame = () => {
    if (currentContextFrameGroup) {
      result.push(currentContextFrameGroup);
      currentContextFrameGroup = null;
    }
  };

  const flushAll = () => {
    flushUnit();
    flushThinking();
    flushContextFrame();
  };

  for (const entry of entries) {
    const cls = classifyEntry(entry);

    switch (cls) {
      case "turn_boundary": {
        flushAll();
        result.push(entry);
        break;
      }

      case "message": {
        if (isEffectivelyEmptyMessage(entry)) {
          // 空消息：完全丢弃，既不切断 unit 也不进入结果
          break;
        }
        flushAll();
        result.push(entry);
        break;
      }

      case "tool_like": {
        flushThinking();
        flushContextFrame();
        if (currentUnit) {
          currentUnit.entries.push(entry);
        } else {
          currentUnit = {
            type: "aggregated_group",
            aggregationType: "turn_fold",
            entries: [entry],
            id: entry.id,
            groupKey: `tool-${entry.id}`,
          };
        }
        break;
      }

      case "thinking": {
        flushUnit();
        flushContextFrame();
        if (currentThinkingGroup) {
          currentThinkingGroup.entries.push(entry);
        } else {
          currentThinkingGroup = {
            type: "aggregated_thinking",
            entries: [entry],
            id: entry.id,
            groupKey: `thinking-${entry.id}`,
          };
        }
        break;
      }

      case "context_frame": {
        flushUnit();
        flushThinking();
        if (currentContextFrameGroup) {
          currentContextFrameGroup.entries.push(entry);
        } else {
          currentContextFrameGroup = {
            type: "aggregated_context_frames",
            entries: [entry],
            id: entry.id,
            groupKey: `context-frame-${entry.id}`,
          };
        }
        break;
      }

      case "non_agg":
      default: {
        flushAll();
        result.push(entry);
        break;
      }
    }
  }

  flushAll();

  return result.map((item) => {
    if (
      (item as AggregatedEntryGroup).type === "aggregated_group" &&
      (item as AggregatedEntryGroup).entries.length === 1
    ) {
      return (item as AggregatedEntryGroup).entries[0]!;
    }
    if (
      (item as AggregatedThinkingGroup).type === "aggregated_thinking" &&
      (item as AggregatedThinkingGroup).entries.length === 1
    ) {
      return (item as AggregatedThinkingGroup).entries[0]!;
    }
    if (
      (item as AggregatedContextFrameGroup).type === "aggregated_context_frames" &&
      (item as AggregatedContextFrameGroup).entries.length === 1
    ) {
      return (item as AggregatedContextFrameGroup).entries[0]!;
    }
    return item;
  });
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
