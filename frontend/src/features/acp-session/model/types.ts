/**
 * 会话事件类型定义
 *
 * 从 backbone-protocol.ts 导入 Codex 对齐的协议类型，
 * 扩展前端特定的显示类型。
 */

export type {
  BackboneEvent,
  BackboneEnvelope,
  ThreadItem,
  PlatformEvent,
  HookTracePayload,
  SourceInfo,
  TraceInfo,
  AgentMessageDeltaNotification,
  ReasoningTextDeltaNotification,
  ReasoningSummaryTextDeltaNotification,
  ItemStartedNotification,
  ItemCompletedNotification,
  CommandExecutionOutputDeltaNotification,
  FileChangeOutputDeltaNotification,
  McpToolCallProgressNotification,
  TurnStartedNotification,
  TurnCompletedNotification,
  TurnDiffUpdatedNotification,
  TurnPlanUpdatedNotification,
  PlanDeltaNotification,
  ThreadTokenUsageUpdatedNotification,
  ThreadStatusChangedNotification,
  ContextCompactedNotification,
  ApprovalRequest,
  ErrorNotification,
  Turn,
  TurnStatus,
  TurnError,
  TurnPlanStep,
  TurnPlanStepStatus,
  ThreadTokenUsage,
  TokenUsageBreakdown,
  CommandExecutionStatus,
  DynamicToolCallStatus,
  McpToolCallStatus,
  PatchApplyStatus,
  UserInput,
  JsonValue,
} from "../../../generated/backbone-protocol";

import type {
  BackboneEvent,
  BackboneEnvelope,
  ThreadItem,
  PlatformEvent,
  ThreadTokenUsage,
} from "../../../generated/backbone-protocol";

// ==================== 前端扩展类型 ====================

export interface SessionEventEnvelope {
  session_id: string;
  event_seq: number;
  notification: BackboneEnvelope;
  occurred_at_ms?: number | null;
  committed_at_ms?: number | null;
  session_update_type?: string | null;
  turn_id?: string | null;
  entry_index?: number | null;
  tool_call_id?: string | null;
}

/** 聚合组子类型（工具调用聚合） */
export type ToolAggregationType =
  | "file_read"
  | "search"
  | "web_fetch"
  | "file_edit"
  | "command_run_read"
  | "command_run_search"
  | "command_run_edit"
  | "command_run_fetch"
  | "info_gather";

/**
 * 显示条目 — entries 数组中的基本单元。
 * 每条 BackboneEvent 归并后对应一个 AcpDisplayEntry。
 */
export interface AcpDisplayEntry {
  id: string;
  sessionId: string;
  timestamp: number;
  eventSeq: number;
  event: BackboneEvent;
  turnId?: string;
  entryIndex?: number;
  isStreaming?: boolean;
  isPendingApproval?: boolean;
  /** delta 累积后的文本（用于 agent_message_delta / reasoning_text_delta 等） */
  accumulatedText?: string;
}

/** 工具调用聚合状态 */
export interface AcpToolCallState {
  itemId: string;
  startedItem: ThreadItem | null;
  completedItem: ThreadItem | null;
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

/** Token 用量信息 */
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

/** 从 BackboneEvent 获取显示文本 */
export function extractTextFromEvent(event: BackboneEvent): string {
  switch (event.type) {
    case "agent_message_delta":
      return event.payload.delta;
    case "reasoning_text_delta":
      return event.payload.delta;
    case "reasoning_summary_delta":
      return event.payload.delta;
    default:
      return "";
  }
}

/** 从 ThreadItem 获取显示标题 */
export function getThreadItemTitle(item: ThreadItem): string {
  switch (item.type) {
    case "commandExecution":
      return item.command;
    case "fileChange":
      return item.changes.length > 0 ? item.changes[0]!.path : "文件变更";
    case "mcpToolCall":
      return `${item.server}/${item.tool}`;
    case "dynamicToolCall":
      return item.tool;
    case "agentMessage":
      return "Agent 消息";
    case "plan":
      return "计划";
    case "reasoning":
      return "推理";
    case "webSearch":
      return item.query;
    case "imageView":
      return item.path;
    case "userMessage":
      return "用户消息";
    default:
      return "未知";
  }
}

/** 从 ThreadItem 获取状态 */
export function getThreadItemStatus(item: ThreadItem): string {
  switch (item.type) {
    case "commandExecution":
      return item.status;
    case "fileChange":
      return item.status;
    case "mcpToolCall":
      return item.status;
    case "dynamicToolCall":
      return item.status;
    default:
      return "completed";
  }
}

/** 从 ThreadItem 获取工具类型标签 */
export function getThreadItemKind(item: ThreadItem): string {
  switch (item.type) {
    case "commandExecution":
      return "execute";
    case "fileChange":
      return "edit";
    case "mcpToolCall":
      return "mcp";
    case "dynamicToolCall":
      return "tool";
    case "webSearch":
      return "search";
    case "imageView":
      return "image";
    case "imageGeneration":
      return "image";
    case "collabAgentToolCall":
      return "collab";
    default:
      return "other";
  }
}

/** 从 BackboneEvent 判断是否是系统/平台事件 */
export function isPlatformEvent(event: BackboneEvent): event is { type: "platform"; payload: PlatformEvent } {
  return event.type === "platform";
}

/** 从 PlatformEvent 中提取 session_meta_update 的 key */
export function getPlatformEventKey(event: PlatformEvent): string | null {
  if (event.kind === "session_meta_update") {
    return event.data.key;
  }
  return event.kind;
}

/** 提取 token 用量信息 */
export function extractTokenUsageFromEvent(event: BackboneEvent): TokenUsageInfo | null {
  if (event.type !== "token_usage_updated") return null;
  const usage: ThreadTokenUsage = event.payload.tokenUsage;
  return {
    inputTokens: usage.total.inputTokens,
    outputTokens: usage.total.outputTokens,
    totalTokens: usage.total.totalTokens,
    maxTokens: usage.modelContextWindow ?? undefined,
    cacheReadTokens: usage.total.cachedInputTokens,
  };
}
