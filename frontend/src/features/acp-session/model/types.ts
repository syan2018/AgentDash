/**
 * ACP (Agent Client Protocol) 会话类型定义
 *
 * 从 @agentclientprotocol/sdk 导入核心类型
 * 扩展前端特定的显示类型
 */

export type {
  SessionId,
  ToolCallId,
  ToolCall,
  ToolCallUpdate,
  ToolCallStatus,
  ToolCallContent,
  ToolCallLocation,
  ToolKind,
  SessionNotification,
  SessionUpdate,
  ContentBlock,
  ContentChunk,
  Content,
  Diff,
  Terminal,
  TextContent,
  ImageContent,
  AudioContent,
  ResourceLink,
  EmbeddedResource,
  TextResourceContents,
  BlobResourceContents,
  Annotations,
  Plan,
  PlanEntry,
  PlanEntryPriority,
  PlanEntryStatus,
  AvailableCommandsUpdate,
  CurrentModeUpdate,
  ConfigOptionUpdate,
  SessionInfoUpdate,
  UsageUpdate,
  RequestPermissionRequest,
  ReadTextFileRequest,
  WriteTextFileRequest,
} from "@agentclientprotocol/sdk";

import type {
  SessionId,
  SessionUpdate,
  ToolCall,
  ToolCallUpdate,
  ToolCallId,
  ToolCallStatus,
  ContentBlock,
} from "@agentclientprotocol/sdk";

// ==================== 前端扩展类型 ====================

/** 聚合组子类型（工具调用聚合） */
export type ToolAggregationType =
  | "file_read"
  | "search"
  | "web_fetch"
  | "file_edit"
  | "command_run_read"
  | "command_run_search"
  | "command_run_edit"
  | "command_run_fetch";

/** ACP 显示条目 */
export interface AcpDisplayEntry {
  id: string;
  sessionId: SessionId;
  timestamp: number;
  update: SessionUpdate;
  /** From `_meta.agentdash.trace.turnId` if present */
  turnId?: string;
  isStreaming?: boolean;
  isPendingApproval?: boolean;
}

/** 工具调用聚合状态 */
export interface AcpToolCallState {
  toolCallId: ToolCallId;
  call: ToolCall | null;
  updates: ToolCallUpdate[];
  finalResult?: unknown;
  status: ToolCallStatus;
}

/** 聚合条目组 */
export interface AggregatedEntryGroup {
  type: "aggregated_group";
  aggregationType: ToolAggregationType;
  entries: AcpDisplayEntry[];
  id: string;
  groupKey: string;
  filePath?: string;
}

/** 思考条目组 */
export interface AggregatedThinkingGroup {
  type: "aggregated_thinking";
  entries: AcpDisplayEntry[];
  id: string;
  groupKey: string;
}

/** 显示条目（单个或聚合） */
export type AcpDisplayItem =
  | AcpDisplayEntry
  | AggregatedEntryGroup
  | AggregatedThinkingGroup;

/** 条目更新回调 */
export type OnEntriesUpdated = (
  entries: AcpDisplayItem[],
  loading: boolean,
) => void;


// ==================== 类型守卫 ====================

export function isAggregatedGroup(
  entry: AcpDisplayItem,
): entry is AggregatedEntryGroup {
  return (entry as AggregatedEntryGroup).type === "aggregated_group";
}

export function isAggregatedThinkingGroup(
  entry: AcpDisplayItem,
): entry is AggregatedThinkingGroup {
  return (entry as AggregatedThinkingGroup).type === "aggregated_thinking";
}

export function isAggregatedDiffGroup(
  entry: AcpDisplayItem,
): entry is AggregatedEntryGroup {
  return isAggregatedGroup(entry) && entry.aggregationType === "file_edit";
}

export function isDisplayEntry(
  entry: AcpDisplayItem,
): entry is AcpDisplayEntry {
  return !isAggregatedGroup(entry) && !isAggregatedThinkingGroup(entry);
}

// ==================== 工具函数 ====================

/** 从 ContentBlock 中提取文本 */
export function extractTextFromContentBlock(content: ContentBlock | undefined): string {
  if (!content) return "";
  if (content.type === "text") {
    return content.text;
  }
  return "";
}
