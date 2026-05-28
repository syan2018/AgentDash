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

export const KIND_REGISTRY: Record<ThreadItemKind, KindMeta> = {
  execute: { kind: "execute", badge: "RUN",  label: "执行",   summaryUnit: "条", summaryVerb: "运行" },
  edit:    { kind: "edit",    badge: "EDIT", label: "编辑",   summaryUnit: "个", summaryVerb: "编辑" },
  read:    { kind: "read",    badge: "READ", label: "读取",   summaryUnit: "个", summaryVerb: "读取" },
  search:  { kind: "search",  badge: "FIND", label: "搜索",   summaryUnit: "次", summaryVerb: "搜索" },
  fetch:   { kind: "fetch",   badge: "FETCH",label: "抓取",   summaryUnit: "次", summaryVerb: "抓取" },
  image:   { kind: "image",   badge: "IMG",  label: "图片",   summaryUnit: "项", summaryVerb: "图片" },
  collab:  { kind: "collab",  badge: "COLL", label: "协作",   summaryUnit: "项", summaryVerb: "协作" },
  context: { kind: "context", badge: "CTX",  label: "上下文", summaryUnit: "次", summaryVerb: "上下文" },
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
    case "fileChange":          return KIND_REGISTRY.edit;
    // MCP 暂复用 TOOL badge，后续若有专门标识再切回 KIND_REGISTRY.mcp
    case "mcpToolCall":         return KIND_REGISTRY.tool;
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
  switch (tool.toLowerCase()) {
    case "read":      return KIND_REGISTRY.read;
    case "write":
    case "edit":
    case "applypatch":
    case "str_replace_editor": return KIND_REGISTRY.edit;
    case "grep":
    case "glob":
    case "websearch":
    case "search":    return KIND_REGISTRY.search;
    case "webfetch":
    case "fetch":     return KIND_REGISTRY.fetch;
    default:          return KIND_REGISTRY.tool;
  }
}
