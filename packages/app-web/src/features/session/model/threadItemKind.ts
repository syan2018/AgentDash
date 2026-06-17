/**
 * ThreadItem kind 单一来源
 *
 * 所有 ThreadItem 渲染相关的 kind 元数据（badge 文案 / 中文 label / 视觉样式键）
 * 集中在此处定义。ToolCallCardShell 和 SessionEntry::buildKindSummary 均从此
 * 注册表读取，不再有散落的字面量重复。
 *
 * 设计意图：未来加新工具种类（或 dynamicToolCall.tool 名细分）时，只在此文件
 * 增补 KIND_REGISTRY + resolveKind/resolveDynamicKind 两处映射，渲染器无需变更。
 */
import type { AgentDashThreadItem } from "../../../generated/backbone-protocol";

export type ThreadItemKind =
  | "execute"
  | "edit"
  | "read"
  | "search"
  | "fetch"
  | "image"
  | "collab"
  | "context"
  | "task"
  | "tool"
  | "mcp"
  | "other";

export interface KindMeta {
  /** kind 标识，作为渲染分支主键 */
  kind: ThreadItemKind;
  /** header badge 文案，大写短词 */
  badge: string;
  /** 中文 label，用于副标题或聚合摘要 */
  label: string;
  /** 聚合摘要量词（"N 条" / "N 个" / "N 项" / "N 次"） */
  summaryUnit: string;
  /** 聚合摘要动词（"运行 / 编辑 / 调用 / 搜索 / ..."） */
  summaryVerb: string;
}

export type DynamicToolFamily =
  | "read"
  | "write"
  | "edit"
  | "grep"
  | "glob"
  | "web_search"
  | "fetch"
  | "todo"
  | "question"
  | "task"
  | "generic";

export interface DynamicToolMeta {
  kind: KindMeta;
  family: DynamicToolFamily;
  fallbackLabel: string;
}

export const KIND_REGISTRY: Record<ThreadItemKind, KindMeta> = {
  execute: { kind: "execute", badge: "RUN",  label: "执行",   summaryUnit: "条", summaryVerb: "运行" },
  edit:    { kind: "edit",    badge: "EDIT", label: "编辑",   summaryUnit: "个", summaryVerb: "编辑" },
  read:    { kind: "read",    badge: "READ", label: "读取",   summaryUnit: "个", summaryVerb: "读取" },
  search:  { kind: "search",  badge: "FIND", label: "搜索",   summaryUnit: "次", summaryVerb: "搜索" },
  fetch:   { kind: "fetch",   badge: "FETCH",label: "抓取",   summaryUnit: "次", summaryVerb: "抓取" },
  image:   { kind: "image",   badge: "IMG",  label: "图片",   summaryUnit: "项", summaryVerb: "图片" },
  collab:  { kind: "collab",  badge: "COLL", label: "协作",   summaryUnit: "项", summaryVerb: "协作" },
  context: { kind: "context", badge: "CTX",  label: "上下文", summaryUnit: "次", summaryVerb: "上下文" },
  task:    { kind: "task",    badge: "TASK", label: "任务",   summaryUnit: "项", summaryVerb: "维护任务" },
  mcp:     { kind: "mcp",     badge: "MCP",  label: "MCP",    summaryUnit: "个", summaryVerb: "调用 MCP" },
  tool:    { kind: "tool",    badge: "TOOL", label: "工具",   summaryUnit: "个", summaryVerb: "调用" },
  other:   { kind: "other",   badge: "TOOL", label: "工具",   summaryUnit: "项", summaryVerb: "其他" },
};

/**
 * 从 ThreadItem 解析 kind 元数据
 *
 * dynamicToolCall 内部按 tool 名做二级解析（Read → read，Grep/Glob/WebSearch → search，
 * Write/Edit → edit，WebFetch → fetch，其余 → tool）。这层细分让前端在后端尚未把
 * ActionType 还原到对应 ThreadItem variant 时也能给出体面的 badge 与 label。
 */
export function resolveKind(item: AgentDashThreadItem): KindMeta {
  switch (item.type) {
    case "commandExecution":    return KIND_REGISTRY.execute;
    case "shellExec":           return KIND_REGISTRY.execute;
    case "fileChange":          return KIND_REGISTRY.edit;
    case "mcpToolCall":         return KIND_REGISTRY.mcp;
    case "webSearch":           return KIND_REGISTRY.search;
    case "imageView":
    case "imageGeneration":     return KIND_REGISTRY.image;
    case "collabAgentToolCall": return KIND_REGISTRY.collab;
    case "contextCompaction":   return KIND_REGISTRY.context;
    case "dynamicToolCall":     return resolveDynamicKind(item.tool);
    case "fsRead":              return KIND_REGISTRY.read;
    case "fsGrep":
    case "fsGlob":              return KIND_REGISTRY.search;
    default:                    return KIND_REGISTRY.other;
  }
}

export function resolveDynamicKind(tool: string): KindMeta {
  return resolveDynamicToolMeta(tool).kind;
}

export function resolveDynamicToolMeta(tool: string): DynamicToolMeta {
  const normalized = tool.toLowerCase();
  switch (normalized) {
    case "read":
      return { kind: KIND_REGISTRY.read, family: "read", fallbackLabel: "Read" };
    case "write":
      return { kind: KIND_REGISTRY.edit, family: "write", fallbackLabel: "Write" };
    case "edit":
      return { kind: KIND_REGISTRY.edit, family: "edit", fallbackLabel: "Edit" };
    case "applypatch":
      return { kind: KIND_REGISTRY.edit, family: "edit", fallbackLabel: "Edit" };
    case "str_replace_editor":
      return { kind: KIND_REGISTRY.edit, family: "edit", fallbackLabel: "Edit" };
    case "grep":
      return { kind: KIND_REGISTRY.search, family: "grep", fallbackLabel: "Grep" };
    case "glob":
      return { kind: KIND_REGISTRY.search, family: "glob", fallbackLabel: "Glob" };
    case "websearch":
    case "search":
      return { kind: KIND_REGISTRY.search, family: "web_search", fallbackLabel: "WebSearch" };
    case "webfetch":
    case "fetch":
      return { kind: KIND_REGISTRY.fetch, family: "fetch", fallbackLabel: "WebFetch" };
    case "todowrite":
      return { kind: KIND_REGISTRY.tool, family: "todo", fallbackLabel: "TodoWrite" };
    case "task_read":
      return { kind: KIND_REGISTRY.task, family: "task", fallbackLabel: "task_read" };
    case "task_write":
      return { kind: KIND_REGISTRY.task, family: "task", fallbackLabel: "task_write" };
    case "askquestion":
    case "askuserquestion":
      return { kind: KIND_REGISTRY.tool, family: "question", fallbackLabel: "AskQuestion" };
    default:
      return { kind: KIND_REGISTRY.tool, family: "generic", fallbackLabel: tool };
  }
}

export function isToolBurstEligible(item: AgentDashThreadItem): boolean {
  switch (item.type) {
    case "commandExecution":
    case "shellExec":
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
      return true;
    default:
      return false;
  }
}
