/**
 * ACP 会话管理 Hook
 *
 * 整合流管理和条目聚合逻辑。
 * 暴露 displayItems（聚合后）、rawEntries、tokenUsage 等供 UI 使用。
 */

import { useMemo, useRef } from "react";
import { useAcpStream } from "./useAcpStream";
import type { SessionUpdate } from "@agentclientprotocol/sdk";
import {
  isAggregatedGroup as isAggregatedGroupItem,
  isAggregatedThinkingGroup as isAggregatedThinkingGroupItem,
} from "./types";
import type {
  AcpDisplayEntry,
  AcpDisplayItem,
  AggregatedEntryGroup,
  AggregatedThinkingGroup,
  SessionEventEnvelope,
  ToolAggregationType,
  TokenUsageInfo,
} from "./types";

export interface UseAcpSessionOptions {
  sessionId: string;
  endpoint?: string;
  enableAggregation?: boolean;
  /** 透传给 useAcpStream：false 时不发起连接 */
  enabled?: boolean;
}

export interface UseAcpSessionResult {
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
  /** ID of the entry currently being streamed (last agent_message_chunk while receiving data), or null */
  streamingEntryId: string | null;
  /** 最新的 token 用量（累计） */
  tokenUsage: TokenUsageInfo | null;
}

function getToolAggregationType(update: SessionUpdate): ToolAggregationType | null {
  if (update.sessionUpdate !== "tool_call" && update.sessionUpdate !== "tool_call_update") {
    return null;
  }

  const kind = "kind" in update ? update.kind : undefined;
  const title = "title" in update ? (update.title ?? "") : "";

  // 读取类操作统一归为 info_gather
  if (kind === "read") return "info_gather";
  if (kind === "search") return "info_gather";
  if (kind === "fetch") return "info_gather";

  // 编辑类保持独立（按文件路径分组）
  if (kind === "edit") return "file_edit";

  // 命令执行：按 title 启发式判断子类型
  if (kind === "execute") {
    const lowerTitle = (title as string).toLowerCase();
    // 读取类命令 → info_gather
    if (lowerTitle.includes("read") || lowerTitle.includes("cat") || lowerTitle.includes("less")) {
      return "info_gather";
    }
    // 搜索类命令 → info_gather
    if (lowerTitle.includes("search") || lowerTitle.includes("grep") || lowerTitle.includes("find")) {
      return "info_gather";
    }
    // 获取类命令 → info_gather
    if (lowerTitle.includes("fetch") || lowerTitle.includes("curl") || lowerTitle.includes("wget")) {
      return "info_gather";
    }
    // 编辑类命令保持独立
    if (lowerTitle.includes("edit") || lowerTitle.includes("sed") || lowerTitle.includes("awk")) {
      return "command_run_edit";
    }
  }
  return null;
}

function isThinkingUpdate(update: SessionUpdate): boolean {
  return update.sessionUpdate === "agent_thought_chunk";
}

function isFileEditUpdate(update: SessionUpdate): boolean {
  if (update.sessionUpdate !== "tool_call" && update.sessionUpdate !== "tool_call_update") {
    return false;
  }
  const kind = "kind" in update ? update.kind : undefined;
  return kind === "edit";
}

function getFilePathFromUpdate(update: SessionUpdate): string | null {
  if (update.sessionUpdate !== "tool_call" && update.sessionUpdate !== "tool_call_update") {
    return null;
  }
  const locations = "locations" in update ? update.locations : undefined;
  if (Array.isArray(locations) && locations.length > 0) {
    return locations[0].path ?? null;
  }
  return null;
}

/** session_info_update 和 usage_update 不参与聚合，直接 pass-through */
function isNonAggregatableEvent(update: SessionUpdate): boolean {
  return (
    update.sessionUpdate === "session_info_update" ||
    update.sessionUpdate === "usage_update" ||
    update.sessionUpdate === "available_commands_update" ||
    update.sessionUpdate === "current_mode_update" ||
    update.sessionUpdate === "config_option_update"
  );
}

function aggregateEntries(entries: AcpDisplayEntry[]): AcpDisplayItem[] {
  const result: AcpDisplayItem[] = [];
  let currentToolGroup: AggregatedEntryGroup | null = null;
  let currentThinkingGroup: AggregatedThinkingGroup | null = null;
  let currentDiffGroup: AggregatedEntryGroup | null = null;

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
  };

  for (const entry of entries) {
    const update = entry.update;

    // 系统事件不聚合
    if (isNonAggregatableEvent(update)) {
      flushGroups();
      result.push(entry);
      continue;
    }

    if (isFileEditUpdate(update)) {
      const filePath = getFilePathFromUpdate(update);
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

    const aggType = getToolAggregationType(update);
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

    if (isThinkingUpdate(update)) {
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

  // 单条条目不需要聚合壳，还原为独立条目以保留完整的卡片渲染
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
    return item;
  });
}

function entryShallowEqual(a: AcpDisplayEntry, b: AcpDisplayEntry): boolean {
  return (
    a.id === b.id &&
    a.eventSeq === b.eventSeq &&
    a.update === b.update &&
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

  return entryShallowEqual(a as AcpDisplayEntry, b as AcpDisplayEntry);
}

export function useAcpSession(options: UseAcpSessionOptions): UseAcpSessionResult {
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
  } = useAcpStream({
    sessionId,
    endpoint,
    enabled,
  });

  const prevDisplayItemsRef = useRef<AcpDisplayItem[]>([]);

  // 该 useMemo 在渲染期间读写 prevDisplayItemsRef 以复用上一次的 group/entry 引用，
  // 使下游 React.memo 能命中——这是主动做的引用稳定化，而不是遗漏的副作用。
  // useMemo 本身在每次 entries/enableAggregation 变化时重新计算，
  // 不会出现"render 被 ref 写穿"导致跳过更新的 case。
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
    if (last.update.sessionUpdate === "agent_message_chunk") return last.id;
    return null;
  }, [isReceiving, entries]);

  // displayItems 经过上面的引用稳定化 useMemo 产出，lint 规则把它视为 ref 来源；
  // 这里的 return 只是把值透传给调用方，关闭规则。
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

export default useAcpSession;
