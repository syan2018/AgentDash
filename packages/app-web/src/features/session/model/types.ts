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
  AgentDashThreadItem,
  PlatformEvent,
  HookTracePayload,
  HookTraceData,
  HookTraceCompletion,
  HookTraceDiagnostic,
  HookTraceInjection,
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
  NormalizedContextUsage,
  ContextUsageSource,
  CommandExecutionStatus,
  DynamicToolCallStatus,
  McpToolCallStatus,
  PatchApplyStatus,
  UserInput,
} from "../../../generated/backbone-protocol";

import type {
  BackboneEvent,
  AgentDashThreadItem,
  PlatformEvent,
  UserInput,
  ThreadTokenUsage,
  TokenUsageBreakdown,
  NormalizedContextUsage,
  ContextUsageSource,
} from "../../../generated/backbone-protocol";
import type { SessionEventResponse } from "../../../generated/session-contracts";
import type { ContextFrame } from "./contextFrame";
import { resolveKind } from "./threadItemKind";

type JsonRecord = Record<string, unknown>;

function isRecord(value: unknown): value is JsonRecord {
  return value != null && typeof value === "object" && !Array.isArray(value);
}

function readStringField(record: JsonRecord, ...keys: string[]): string | null {
  for (const key of keys) {
    const value = record[key];
    if (typeof value === "string" && value.trim().length > 0) {
      return value;
    }
  }
  return null;
}

function readOptionalNumberField(record: JsonRecord, ...keys: string[]): number | undefined {
  for (const key of keys) {
    const value = record[key];
    if (typeof value === "number" && Number.isFinite(value)) {
      return value;
    }
  }
  return undefined;
}

function pickNameFromUri(uri: string): string {
  const normalized = uri.replace(/\\/g, "/");
  const parts = normalized.split("/");
  return parts[parts.length - 1] || uri;
}

export type TextResourceContents = {
  uri: string;
  mimeType?: string | null;
  text: string;
};

export type BlobResourceContents = {
  uri: string;
  mimeType?: string | null;
  blob: string;
};

export type EmbeddedResource = TextResourceContents | BlobResourceContents;

export type ContentBlock =
  | { type: "text"; text: string }
  | { type: "resource_link"; uri: string; name?: string; mimeType?: string | null; size?: number | null }
  | { type: "resource"; resource: EmbeddedResource }
  | { type: "image"; data: string; mimeType?: string | null }
  | { type: "audio"; data: string; mimeType?: string | null };

/**
 * 把 platform.session_meta_update.value 解析为可渲染的输入块。
 * 仅按当前 Backbone 链路下发的 block 结构解析。
 */
export function parseContentBlock(value: unknown): ContentBlock | null {
  if (!isRecord(value)) return null;

  const type = readStringField(value, "type");
  if (!type) return null;

  switch (type) {
    case "text": {
      const text = readStringField(value, "text");
      if (text == null) return null;
      return { type: "text", text };
    }

    case "resource_link": {
      const uri = readStringField(value, "uri");
      if (!uri) return null;
      const name = readStringField(value, "name");
      const mimeType = readStringField(value, "mimeType") ?? undefined;
      const size = readOptionalNumberField(value, "size");
      return {
        type: "resource_link",
        uri,
        name: name ?? undefined,
        mimeType,
        size,
      };
    }

    case "resource": {
      const resourceValue = value.resource;
      if (!isRecord(resourceValue)) return null;
      const uri = readStringField(resourceValue, "uri");
      if (!uri) return null;
      const mimeType = readStringField(resourceValue, "mimeType") ?? undefined;
      const text = readStringField(resourceValue, "text");
      if (text != null) {
        return {
          type: "resource",
          resource: { uri, mimeType, text },
        };
      }
      const blob = readStringField(resourceValue, "blob");
      if (blob != null) {
        return {
          type: "resource",
          resource: { uri, mimeType, blob },
        };
      }
      return null;
    }

    case "image":
    case "audio": {
      const data = readStringField(value, "data");
      if (!data) return null;
      const mimeType = readStringField(value, "mimeType") ?? undefined;
      if (type === "image") {
        return { type: "image", data, mimeType };
      }
      return { type: "audio", data, mimeType };
    }

    default:
      return null;
  }
}

/** 从输入块提取兜底文本（用于无专用卡片时展示）。 */
export function extractTextFromContentBlock(content: ContentBlock | null | undefined): string {
  if (!content) return "";

  switch (content.type) {
    case "text":
      return content.text;

    case "resource_link": {
      const label = content.name?.trim() || pickNameFromUri(content.uri) || content.uri;
      if (!content.uri || label === content.uri) {
        return `📎 引用文件: ${label}`;
      }
      return `📎 引用文件: ${label}\n${content.uri}`;
    }

    case "resource": {
      const resource = content.resource;
      if ("text" in resource) {
        const label = pickNameFromUri(resource.uri);
        const mimeText = resource.mimeType ? ` (${resource.mimeType})` : "";
        return `📎 引用文件内容: ${label}${mimeText}\n${resource.uri}\n（已附带 ${resource.text.length} 字符）`;
      }
      const label = pickNameFromUri(resource.uri);
      const mimeText = resource.mimeType ? ` (${resource.mimeType})` : "";
      return `📎 引用二进制资源: ${label}${mimeText}\n${resource.uri}`;
    }

    case "image":
      return content.mimeType ? `🖼️ 图片内容 (${content.mimeType})` : "🖼️ 图片内容";

    case "audio":
      return content.mimeType ? `🔊 音频内容 (${content.mimeType})` : "🔊 音频内容";
  }
}

export function extractTextFromUserInput(input: UserInput): string {
  switch (input.type) {
    case "text":
      return input.text;
    case "image":
      return `引用图片: ${input.url}`;
    case "localImage":
      return `引用本地图片: ${input.path}`;
    case "skill":
      return `引用 Skill: ${input.name}`;
    case "mention":
      return `引用: ${input.name}`;
  }
}

export function extractTextFromUserInputs(input: readonly UserInput[]): string {
  return input
    .map(extractTextFromUserInput)
    .map((text) => text.trim())
    .filter((text) => text.length > 0)
    .join("\n");
}

/** 用户消息中的图片块（用专有 block 渲染，而非拍扁成文本）。 */
export interface UserMessageImage {
  /** 可直接作为 <img src> 的地址（data URL 或远程 URL）。 */
  url: string;
  /** 无障碍文本 / lightbox 标题。 */
  alt: string;
}

/** 用户输入拆分结果：文本部分仍走文本气泡，图片部分走专有图片 block。 */
export interface PartitionedUserInputs {
  text: string;
  images: UserMessageImage[];
}

/**
 * 将用户输入拆为「文本」与「图片」两部分。
 * image 块（含 data URL）单独成图片 block 渲染，避免把 base64 拍扁成文本；
 * 其余块（text / localImage / skill / mention）仍按既有文本语义拼接。
 */
export function partitionUserInputs(input: readonly UserInput[]): PartitionedUserInputs {
  const images: UserMessageImage[] = [];
  const textParts: string[] = [];

  for (const block of input) {
    if (block.type === "image") {
      images.push({ url: block.url, alt: `用户图片 ${images.length + 1}` });
      continue;
    }
    const text = extractTextFromUserInput(block).trim();
    if (text.length > 0) textParts.push(text);
  }

  return { text: textParts.join("\n"), images };
}

// ==================== 前端扩展类型 ====================

export type SessionEventEnvelope = SessionEventResponse & {
  /** 进度态事件标记：仅 live 显示，不写入可重放 rawEvents backlog。 */
  ephemeral?: boolean;
};

/** UI 时间线顺序来源：durable 与 ephemeral progress 使用不同坐标。 */
export type TimelineOrder =
  | { kind: "durable"; seq: number }
  | { kind: "anchored_progress"; anchorId: string; progressSeq: number }
  | { kind: "local_progress"; receivedOrdinal: number; progressSeq: number };

/** 同一 item 的事实新鲜度，避免低权威事件回写高权威 UI 状态。 */
export type SessionItemFreshness = "started" | "progress" | "completed";

/** 聚合组子类型（工具调用聚合） */
export type ToolAggregationType =
  | "tool_burst"
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
 * 每条 BackboneEvent 归并后对应一个 SessionDisplayEntry。
 */
export interface SessionDisplayEntry {
  id: string;
  sessionId: string;
  timestamp: number;
  /** durable event seq；对纯 ephemeral entry 仅作诊断值，不参与 durable 时间线排序。 */
  eventSeq: number;
  /** UI 排序事实源，拆开 durable event_seq 与 ephemeral progress_seq。 */
  timelineOrder?: TimelineOrder;
  /** 最近应用到该 entry 的 ephemeral_seq，只用于 progress 去重/诊断。 */
  progressSeq?: number;
  /** 同 item lifecycle freshness：completed > progress > started。 */
  itemFreshness?: SessionItemFreshness;
  event: BackboneEvent;
  turnId?: string;
  entryIndex?: number;
  isStreaming?: boolean;
  isPendingApproval?: boolean;
  /** delta 累积后的文本（用于 agent_message_delta / reasoning_text_delta 等） */
  accumulatedText?: string;
  /** model 层解析后的 context frame，供 UI 直接渲染。 */
  contextFrame?: ContextFrame;
  /** AgentRun conversation feed 投影消息：只用于 UI 分段稳定性，不是 runtime event fact。 */
  projectedTranscriptStable?: boolean;
}

/** 工具调用聚合状态 */
export interface SessionToolCallState {
  itemId: string;
  startedItem: AgentDashThreadItem | null;
  completedItem: AgentDashThreadItem | null;
  status: string;
}

/** 聚合条目组 */
export interface AggregatedEntryGroup {
  type: "aggregated_group";
  aggregationType: ToolAggregationType;
  entries: SessionDisplayEntry[];
  id: string;
  groupKey: string;
}

/** 思考条目组 */
export interface AggregatedThinkingGroup {
  type: "aggregated_thinking";
  entries: SessionDisplayEntry[];
  id: string;
  groupKey: string;
  turnId?: string;
  eventSeq: number;
  isStreamingThinking?: boolean;
}

/** 相邻 ContextFrame 的用户侧聚合组 */
export interface AggregatedContextFrameGroup {
  type: "aggregated_context_frames";
  entries: SessionDisplayEntry[];
  id: string;
  groupKey: string;
}

/** 显示条目（单个或聚合） */
export type SessionDisplayItem =
  | SessionDisplayEntry
  | AggregatedEntryGroup
  | AggregatedThinkingGroup
  | AggregatedContextFrameGroup;

/** 条目更新回调 */
export type OnEntriesUpdated = (
  entries: SessionDisplayItem[],
  loading: boolean,
) => void;

export interface TokenUsageBreakdownInfo {
  inputTokens: number;
  outputTokens: number;
  totalTokens: number;
  cacheReadTokens: number;
  cacheCreationTokens: number;
  reasoningTokens: number;
}

export interface NormalizedContextUsageInfo {
  providerContextTokens: number;
  pendingEstimateTokens: number;
  currentContextTokens: number;
  cumulativeTotalTokens: number;
  modelContextWindow?: number;
  effectiveContextWindow?: number;
  reserveTokens: number;
  source: ContextUsageSource;
}

/** Token 用量信息 */
export interface TokenUsageInfo {
  /** 当前上下文压力，用于上下文环和压缩判断展示。 */
  currentContextTokens: number;
  /** 最近 provider usage 可确认的上下文占用。 */
  providerContextTokens: number;
  /** 最近 provider usage 后新增内容的本地估算。 */
  pendingEstimateTokens: number;
  /** 累计 session 消耗，用于统计展示，不参与上下文窗口判断。 */
  cumulativeTotalTokens: number;
  modelContextWindow?: number;
  effectiveContextWindow?: number;
  reserveTokens: number;
  usageSource: ContextUsageSource;
  last: TokenUsageBreakdownInfo;
  total: TokenUsageBreakdownInfo;
}

// ==================== 类型守卫 ====================

export function isAggregatedGroup(
  entry: SessionDisplayItem,
): entry is AggregatedEntryGroup {
  return (entry as AggregatedEntryGroup).type === "aggregated_group";
}

export function isAggregatedThinkingGroup(
  entry: SessionDisplayItem,
): entry is AggregatedThinkingGroup {
  return (entry as AggregatedThinkingGroup).type === "aggregated_thinking";
}

export function isAggregatedContextFrameGroup(
  entry: SessionDisplayItem,
): entry is AggregatedContextFrameGroup {
  return (entry as AggregatedContextFrameGroup).type === "aggregated_context_frames";
}

// 历史保留；新算法不再产出 file_edit 类型
export function isAggregatedDiffGroup(
  entry: SessionDisplayItem,
): entry is AggregatedEntryGroup {
  return isAggregatedGroup(entry) && entry.aggregationType === "file_edit";
}

export function isDisplayEntry(
  entry: SessionDisplayItem,
): entry is SessionDisplayEntry {
  return !isAggregatedGroup(entry) &&
    !isAggregatedThinkingGroup(entry) &&
    !isAggregatedContextFrameGroup(entry);
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

/** 从 ThreadItem / AgentDashThreadItem 获取显示标题 */
export function getThreadItemTitle(item: AgentDashThreadItem): string {
  switch (item.type) {
    case "commandExecution":
    case "shellExec":
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
    case "contextCompaction":
      return "上下文压缩";
    case "fsRead":
      return `Read ${item.path}`;
    case "fsGrep":
      return `Grep "${item.pattern}"`;
    case "fsGlob":
      return `Glob ${item.pattern}`;
    default:
      return "未知";
  }
}

/** 从 ThreadItem / AgentDashThreadItem 获取状态 */
export function getThreadItemStatus(item: AgentDashThreadItem): string {
  switch (item.type) {
    case "commandExecution":
    case "shellExec":
    case "fileChange":
    case "mcpToolCall":
    case "dynamicToolCall":
    case "collabAgentToolCall":
    case "fsRead":
    case "fsGrep":
    case "fsGlob":
      return item.status;
    case "contextCompaction":
      return "completed";
    default:
      return "completed";
  }
}

/** 从 ThreadItem / AgentDashThreadItem 获取工具类型标签（委托给 threadItemKind 注册表） */
export function getThreadItemKind(item: AgentDashThreadItem): string {
  return resolveKind(item).kind;
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
  const context = normalizeContextUsage(usage.context, usage);
  return {
    currentContextTokens: context.currentContextTokens,
    providerContextTokens: context.providerContextTokens,
    pendingEstimateTokens: context.pendingEstimateTokens,
    cumulativeTotalTokens: context.cumulativeTotalTokens,
    modelContextWindow: context.modelContextWindow,
    effectiveContextWindow: context.effectiveContextWindow,
    reserveTokens: context.reserveTokens,
    usageSource: context.source,
    last: normalizeTokenBreakdown(usage.last),
    total: normalizeTokenBreakdown(usage.total),
  };
}

function positiveNumberOrUndefined(value: number | null | undefined): number | undefined {
  if (value == null || !Number.isFinite(value) || value <= 0) return undefined;
  return value;
}

function nonNegativeNumber(value: number | null | undefined): number {
  if (value == null || !Number.isFinite(value) || value < 0) return 0;
  return value;
}

function normalizeTokenBreakdown(value: TokenUsageBreakdown): TokenUsageBreakdownInfo {
  return {
    inputTokens: nonNegativeNumber(value.inputTokens),
    outputTokens: nonNegativeNumber(value.outputTokens),
    totalTokens: nonNegativeNumber(value.totalTokens),
    cacheReadTokens: nonNegativeNumber(value.cachedInputTokens),
    cacheCreationTokens: 0,
    reasoningTokens: nonNegativeNumber(value.reasoningOutputTokens),
  };
}

function normalizeContextUsage(
  value: NormalizedContextUsage,
  usage: ThreadTokenUsage,
): NormalizedContextUsageInfo {
  const modelContextWindow = positiveNumberOrUndefined(value.modelContextWindow ?? usage.modelContextWindow);
  const effectiveContextWindow = positiveNumberOrUndefined(value.effectiveContextWindow) ?? modelContextWindow;
  const providerContextTokens = nonNegativeNumber(value.providerContextTokens);
  const pendingEstimateTokens = nonNegativeNumber(value.pendingEstimateTokens);
  const currentContextTokens = nonNegativeNumber(value.currentContextTokens);
  const cumulativeTotalTokens = nonNegativeNumber(value.cumulativeTotalTokens);

  return {
    providerContextTokens,
    pendingEstimateTokens,
    currentContextTokens,
    cumulativeTotalTokens,
    modelContextWindow,
    effectiveContextWindow,
    reserveTokens: nonNegativeNumber(value.reserveTokens),
    source: value.source,
  };
}
