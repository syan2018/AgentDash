/**
 * ACP 会话管理 Hook
 *
 * 整合流管理和条目聚合逻辑
 */

import { useCallback, useMemo } from "react";
import { useAcpStream } from "./useAcpStream";
import type { SessionUpdate } from "@agentclientprotocol/sdk";
import type {
  AcpDisplayEntry,
  AcpDisplayItem,
  AggregatedEntryGroup,
  AggregatedThinkingGroup,
  ToolAggregationType,
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
  isConnected: boolean;
  isLoading: boolean;
  error: Error | null;
  reconnect: () => void;
  close: () => void;
  sendCancel: () => void;
  /** ID of the entry currently being streamed (last agent_message_chunk while receiving data), or null */
  streamingEntryId: string | null;
}

function getToolAggregationType(update: SessionUpdate): ToolAggregationType | null {
  if (update.sessionUpdate !== "tool_call" && update.sessionUpdate !== "tool_call_update") {
    return null;
  }

  const kind = "kind" in update ? update.kind : undefined;
  const title = "title" in update ? (update.title ?? "") : "";

  if (kind === "read") return "file_read";
  if (kind === "search") return "search";
  if (kind === "fetch") return "web_fetch";
  if (kind === "edit") return "file_edit";
  if (kind === "execute") {
    const lowerTitle = (title as string).toLowerCase();
    if (lowerTitle.includes("read") || lowerTitle.includes("cat") || lowerTitle.includes("less")) {
      return "command_run_read";
    }
    if (lowerTitle.includes("search") || lowerTitle.includes("grep") || lowerTitle.includes("find")) {
      return "command_run_search";
    }
    if (lowerTitle.includes("edit") || lowerTitle.includes("sed") || lowerTitle.includes("awk")) {
      return "command_run_edit";
    }
    if (lowerTitle.includes("fetch") || lowerTitle.includes("curl") || lowerTitle.includes("wget")) {
      return "command_run_fetch";
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

    // 处理文件编辑聚合（优先于通用工具聚合）
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

    // 处理工具调用聚合
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

    // 处理思考消息聚合
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

    // 非聚合条目
    flushGroups();
    result.push(entry);
  }

  // 结束剩余的聚合组
  flushGroups();

  return result;
}

export function useAcpSession(options: UseAcpSessionOptions): UseAcpSessionResult {
  const { sessionId, endpoint, enableAggregation = true, enabled } = options;

  const handleEntry = useCallback(() => {
    // 条目更新时触发重新渲染
  }, []);

  const {
    entries,
    isConnected,
    isLoading,
    isReceiving,
    error,
    reconnect,
    close,
    sendCancel,
  } = useAcpStream({
    sessionId,
    endpoint,
    enabled,
    onEntry: handleEntry,
  });

  const displayItems = useMemo(() => {
    if (!enableAggregation) {
      return entries as AcpDisplayItem[];
    }
    return aggregateEntries(entries);
  }, [entries, enableAggregation]);

  // Streaming indicator: only the last agent_message_chunk while actively receiving data
  const streamingEntryId = useMemo(() => {
    if (!isReceiving || entries.length === 0) return null;
    const last = entries[entries.length - 1]!;
    if (last.update.sessionUpdate === "agent_message_chunk") return last.id;
    return null;
  }, [isReceiving, entries]);

  return {
    displayItems,
    rawEntries: entries,
    isConnected,
    isLoading,
    error,
    reconnect,
    close,
    sendCancel,
    streamingEntryId,
  };
}

export default useAcpSession;
