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
  ToolCallId,
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

/**
 * ACP 显示条目 — entries 数组中的基本单元。
 * 每个 ACP SessionNotification 归并后都对应一个 AcpDisplayEntry。
 */
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
  call: SessionUpdate | null;
  updates: SessionUpdate[];
  finalResult?: unknown;
  status: string;
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

/** Token 用量信息（从 usage_update 事件提取） */
export interface TokenUsageInfo {
  inputTokens?: number;
  outputTokens?: number;
  totalTokens?: number;
  maxTokens?: number;
  cacheReadTokens?: number;
  cacheCreationTokens?: number;
}

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

/** 从 SessionUpdate 判断是否是系统事件（session_info_update）*/
export function isSystemEvent(update: SessionUpdate): boolean {
  return update.sessionUpdate === "session_info_update";
}

/** 从 SessionUpdate 判断是否是用量事件（usage_update）*/
export function isUsageEvent(update: SessionUpdate): boolean {
  return update.sessionUpdate === "usage_update";
}
