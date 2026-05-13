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
  AcpDisplayEntry,
  AcpDisplayItem,
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
  displayItems: AcpDisplayItem[];
  rawEntries: AcpDisplayEntry[];
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
    case "commandExecution": {
      const cmd = item.command.toLowerCase();
      if (cmd.includes("cat") || cmd.includes("less") || cmd.includes("head") || cmd.includes("tail")) {
        return "info_gather";
      }
      if (cmd.includes("grep") || cmd.includes("find") || cmd.includes("rg")) {
        return "info_gather";
      }
      if (cmd.includes("curl") || cmd.includes("wget") || cmd.includes("fetch")) {
        return "info_gather";
      }
      if (cmd.includes("sed") || cmd.includes("awk")) {
        return "command_run_edit";
      }
      return null;
    }
    case "fileChange":
      return "file_edit";
    case "mcpToolCall":
    case "dynamicToolCall":
      return "info_gather";
    case "webSearch":
      return "info_gather";
    default:
      return null;
  }
}

function isThinkingEvent(event: BackboneEvent): boolean {
  return event.type === "reasoning_text_delta" || event.type === "reasoning_summary_delta";
}

function isFileEditEvent(event: BackboneEvent): boolean {
  const item = extractThreadItem(event);
  return item?.type === "fileChange";
}

function isContextFrameEvent(event: BackboneEvent): boolean {
  return (
    event.type === "platform" &&
    event.payload.kind === "session_meta_update" &&
    event.payload.data.key === "context_frame"
  );
}

function getFilePathFromEvent(event: BackboneEvent): string | null {
  const item = extractThreadItem(event);
  if (item?.type === "fileChange" && item.changes.length > 0) {
    return item.changes[0]!.path;
  }
  return null;
}

function isNonAggregatableEvent(event: BackboneEvent): boolean {
  return (
    event.type === "platform" ||
    event.type === "token_usage_updated" ||
    event.type === "thread_status_changed" ||
    event.type === "turn_started" ||
    event.type === "turn_completed" ||
    event.type === "error" ||
    event.type === "approval_request"
  );
}

function aggregateEntries(entries: AcpDisplayEntry[]): AcpDisplayItem[] {
  const result: AcpDisplayItem[] = [];
  let currentToolGroup: AggregatedEntryGroup | null = null;
  let currentThinkingGroup: AggregatedThinkingGroup | null = null;
  let currentDiffGroup: AggregatedEntryGroup | null = null;
  let currentContextFrameGroup: AggregatedContextFrameGroup | null = null;

  const flushGroups = () => {
    if (currentToolGroup) {
      result.push(currentToolGroup);
      currentToolGroup = null;
    }
    if (currentThinkingGroup) {
      result.push(currentThinkingGroup);
      currentThinkingGroup = null;
    }
    if (currentDiffGroup) {
      result.push(currentDiffGroup);
      currentDiffGroup = null;
    }
    if (currentContextFrameGroup) {
      result.push(currentContextFrameGroup);
      currentContextFrameGroup = null;
    }
  };

  for (const entry of entries) {
    const event = entry.event;

    if (isContextFrameEvent(event)) {
      if (currentContextFrameGroup) {
        currentContextFrameGroup.entries.push(entry);
      } else {
        flushGroups();
        currentContextFrameGroup = {
          type: "aggregated_context_frames",
          entries: [entry],
          id: entry.id,
          groupKey: `context-frame-${entry.id}`,
        };
      }
      continue;
    }

    if (isNonAggregatableEvent(event)) {
      flushGroups();
      result.push(entry);
      continue;
    }

    if (isFileEditEvent(event)) {
      const filePath = getFilePathFromEvent(event);
      if (filePath) {
        if (currentDiffGroup && currentDiffGroup.filePath === filePath) {
          currentDiffGroup.entries.push(entry);
        } else {
          flushGroups();
          currentDiffGroup = {
            type: "aggregated_group",
            aggregationType: "file_edit",
            entries: [entry],
            id: entry.id,
            groupKey: `diff-${entry.id}`,
            filePath,
          };
        }
        continue;
      }
    }

    const aggType = getToolAggregationType(event);
    if (aggType && aggType !== "file_edit") {
      if (currentToolGroup && currentToolGroup.aggregationType === aggType) {
        currentToolGroup.entries.push(entry);
      } else {
        flushGroups();
        currentToolGroup = {
          type: "aggregated_group",
          aggregationType: aggType,
          entries: [entry],
          id: entry.id,
          groupKey: `tool-${entry.id}`,
        };
      }
      continue;
    }

    if (isThinkingEvent(event)) {
      if (currentThinkingGroup) {
        currentThinkingGroup.entries.push(entry);
      } else {
        flushGroups();
        currentThinkingGroup = {
          type: "aggregated_thinking",
          entries: [entry],
          id: entry.id,
          groupKey: `thinking-${entry.id}`,
        };
      }
      continue;
    }

    flushGroups();
    result.push(entry);
  }

  flushGroups();

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

function entryShallowEqual(a: AcpDisplayEntry, b: AcpDisplayEntry): boolean {
  return (
    a.id === b.id &&
    a.eventSeq === b.eventSeq &&
    a.event === b.event &&
    a.isPendingApproval === b.isPendingApproval
  );
}

function isAggregatedGroupEqual(a: AcpDisplayItem, b: AcpDisplayItem): boolean {
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
    if (ga.filePath !== gb.filePath) return false;
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

  return entryShallowEqual(a as AcpDisplayEntry, b as AcpDisplayEntry);
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

  const prevDisplayItemsRef = useRef<AcpDisplayItem[]>([]);

  /* eslint-disable react-hooks/refs */
  const displayItems = useMemo(() => {
    const next: AcpDisplayItem[] = enableAggregation
      ? aggregateEntries(entries)
      : (entries as AcpDisplayItem[]);

    const prev = prevDisplayItemsRef.current;
    if (prev.length === next.length) {
      let allEqual = true;
      const stabilized: AcpDisplayItem[] = new Array(next.length);
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
