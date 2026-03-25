/**
 * ACP 会话管理 Hook
 *
 * 整合流管理和条目聚合逻辑。
 * 暴露 displayItems（聚合后）、rawEntries、tokenUsage 等供 UI 使用。
 */

import { useMemo } from "react";
import { useAcpStream } from "./useAcpStream";
import type { SessionUpdate } from "@agentclientprotocol/sdk";
import type {
  AcpDisplayEntry,
  AcpDisplayItem,
  AggregatedEntryGroup,
  AggregatedThinkingGroup,
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

  return result;
}

export function useAcpSession(options: UseAcpSessionOptions): UseAcpSessionResult {
  const { sessionId, endpoint, enableAggregation = true, enabled } = options;

  const {
    entries,
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

  const displayItems = useMemo(() => {
    if (!enableAggregation) {
      return entries as AcpDisplayItem[];
    }
    return aggregateEntries(entries);
  }, [entries, enableAggregation]);

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
