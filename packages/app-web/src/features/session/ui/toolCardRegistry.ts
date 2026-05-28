/**
 * ThreadItem → renderer 一级分发注册表
 *
 * 根据 ThreadItem.type 选择对应的 renderer，返回 { kind, header, body, status }，
 * 由 ToolCallCardShell 包裹渲染。
 *
 * dynamicToolCall 内部按 tool 名做二级 header 摘要。
 */

import { createElement, type ReactNode } from "react";
import type { ThreadItem, AgentDashThreadItem } from "../../../generated/backbone-protocol";
import { resolveKind, type KindMeta } from "../model/threadItemKind";
import type { DisplayStatus } from "./ToolCallCardShell";
import type { ToolCardHeaderModel } from "./ToolCardHeader";
import { FilePathPill } from "./FilePathPill";

import { CommandExecutionCardBody } from "./bodies/CommandExecutionCardBody";
import { FileChangeCardBody } from "./bodies/FileChangeCardBody";
import { McpCardBody } from "./bodies/McpCardBody";
import { WebSearchCardBody } from "./bodies/WebSearchCardBody";
import { ImageCardBody } from "./bodies/ImageCardBody";
import { CollabAgentCardBody } from "./bodies/CollabAgentCardBody";
import { ContextCompactionCardBody } from "./bodies/ContextCompactionCardBody";
import { DynamicToolCallCardBody } from "./bodies/DynamicToolCallCardBody";
import { GenericJsonBody } from "./bodies/GenericJsonBody";
import { ReadCardBody } from "./bodies/ReadCardBody";

export interface CardContext {
  sessionId?: string;
  outputText?: string;
}

export interface CardRenderResult {
  kind: KindMeta;
  header: ToolCardHeaderModel;
  body: ReactNode;
  status: DisplayStatus;
  durationMs?: number;
}

export function renderToolCallCard(
  item: AgentDashThreadItem,
  ctx: CardContext,
): CardRenderResult {
  const kind = resolveKind(item);
  const status = getItemDisplayStatus(item);

  switch (item.type) {
    case "commandExecution":
      return {
        kind,
        header: {
          primary: createElement("code", { className: "font-mono" }, item.command),
          secondary: item.cwd ? `cwd: ${item.cwd}` : undefined,
        },
        body: createElement(CommandExecutionCardBody, {
          item,
          outputText: ctx.outputText,
          sessionId: ctx.sessionId,
        }),
        status,
        durationMs: item.durationMs ?? undefined,
      };

    case "fileChange": {
      const stats = sumDiffStats(item.changes);
      const n = item.changes.length;
      const firstPath = item.changes[0]?.path ?? "";
      return {
        kind,
        header: {
          primary: firstPath ? createElement(FilePathPill, { path: firstPath }) : "文件变更",
          secondary:
            n === 0
              ? undefined
              : n > 1
                ? `+${n - 1} 文件 · +${stats.added} -${stats.removed}`
                : `+${stats.added} -${stats.removed}`,
        },
        body: createElement(FileChangeCardBody, { item }),
        status,
      };
    }

    case "mcpToolCall":
      return {
        kind,
        header: {
          primary: `${item.server}/${item.tool}`,
          secondary: summarizeArgs(item.arguments),
        },
        body: createElement(McpCardBody, { item }),
        status,
        durationMs: item.durationMs ?? undefined,
      };

    case "webSearch":
      return {
        kind,
        header: { primary: createElement("code", { className: "font-mono" }, `"${item.query}"`) },
        body: createElement(WebSearchCardBody, { item }),
        status,
      };

    case "imageView":
      return {
        kind,
        header: { primary: createElement(FilePathPill, { path: item.path }) },
        body: createElement(ImageCardBody, { item }),
        status,
      };

    case "imageGeneration":
      return {
        kind,
        header: { primary: "图片生成" },
        body: createElement(ImageCardBody, { item }),
        status,
      };

    case "collabAgentToolCall":
      return {
        kind,
        header: { primary: item.tool, secondary: "协作 agent" },
        body: createElement(CollabAgentCardBody, { item }),
        status,
      };

    case "contextCompaction":
      return {
        kind,
        header: { primary: "上下文压缩" },
        body: createElement(ContextCompactionCardBody),
        status: status === "inProgress" ? "inProgress" : "completed",
      };

    case "dynamicToolCall":
      return {
        kind,
        header: getDynamicToolHeader(item),
        body: createElement(DynamicToolCallCardBody, { item }),
        status,
        durationMs: item.durationMs ?? undefined,
      };

    case "fsRead":
      return {
        kind,
        header: {
          primary: createElement(FilePathPill, {
            path: item.path,
            range: rangeOf(item.offset, item.limit),
          }),
        },
        body: createElement(ReadCardBody, { item }),
        status,
      };

    case "fsGrep":
      return {
        kind,
        header: {
          primary: createElement("code", { className: "font-mono" }, `"${item.pattern}"`),
          secondary: (item.path ?? item.glob) ? `in ${item.path ?? item.glob}` : undefined,
        },
        body: createElement(GenericJsonBody, {
          arguments: item.arguments,
          contentItems: item.contentItems,
        }),
        status,
      };

    case "fsGlob":
      return {
        kind,
        header: {
          primary: createElement("code", { className: "font-mono" }, item.pattern),
        },
        body: createElement(GenericJsonBody, {
          arguments: item.arguments,
          contentItems: item.contentItems,
        }),
        status,
      };

    default:
      return { kind, header: { primary: "未知" }, body: null, status };
  }
}

// ── dynamicToolCall 二级 header ──

type DynamicItem = Extract<ThreadItem, { type: "dynamicToolCall" }>;

function getDynamicToolHeader(item: DynamicItem): ToolCardHeaderModel {
  const args = item.arguments as Record<string, unknown> | null;
  const tool = item.tool.toLowerCase();

  switch (tool) {
    case "read": {
      const path = str(args, "path") ?? str(args, "file_path");
      if (!path) return { primary: "Read" };
      return {
        primary: createElement(FilePathPill, {
          path,
          range: rangeOf(num(args, "offset"), num(args, "limit")),
        }),
      };
    }
    case "write": {
      const path = str(args, "file_path") ?? str(args, "path");
      return {
        primary: path ? createElement(FilePathPill, { path }) : "Write",
      };
    }
    case "edit":
    case "str_replace_editor":
    case "applypatch": {
      const path = str(args, "file_path") ?? str(args, "path");
      return {
        primary: path ? createElement(FilePathPill, { path }) : "Edit",
      };
    }
    case "grep": {
      const pattern = str(args, "pattern");
      const target = str(args, "path") ?? str(args, "glob");
      return {
        primary: pattern
          ? createElement("code", { className: "font-mono" }, `"${pattern}"`)
          : "Grep",
        secondary: target ? `in ${target}` : undefined,
      };
    }
    case "glob": {
      const pattern = str(args, "pattern") ?? str(args, "glob_pattern");
      return {
        primary: pattern
          ? createElement("code", { className: "font-mono" }, pattern)
          : "Glob",
      };
    }
    case "websearch": {
      const query = str(args, "search_term") ?? str(args, "query");
      return {
        primary: query
          ? createElement("code", { className: "font-mono" }, `"${query}"`)
          : "WebSearch",
      };
    }
    case "webfetch":
    case "fetch": {
      const url = str(args, "url");
      return { primary: url ?? "WebFetch" };
    }
    case "todowrite": {
      const todos = args?.todos;
      const count = Array.isArray(todos) ? todos.length : 0;
      return { primary: count > 0 ? `更新 ${count} 项 todo` : "TodoWrite" };
    }
    case "askquestion":
    case "askuserquestion": {
      const questions = args?.questions;
      const first =
        Array.isArray(questions) && questions[0]
          ? ((questions[0] as Record<string, unknown>).prompt ??
            (questions[0] as Record<string, unknown>).question)
          : null;
      const q = typeof first === "string" ? first : null;
      const n = Array.isArray(questions) ? questions.length : 0;
      return {
        primary: q ?? "AskQuestion",
        secondary: n > 1 ? `+${n - 1} 个问题` : undefined,
      };
    }
    default: {
      const ns = item.namespace;
      return {
        primary: ns ? `${ns}/${item.tool}` : item.tool,
        secondary: summarizeArgs(args),
      };
    }
  }
}

// ── 工具函数 ──

function rangeOf(
  offset: number | null | undefined,
  limit: number | null | undefined,
): { from: number; to: number } | null {
  if (offset == null || limit == null) return null;
  return { from: offset, to: offset + limit };
}

function sumDiffStats(
  changes: Extract<ThreadItem, { type: "fileChange" }>["changes"],
): { added: number; removed: number } {
  let added = 0;
  let removed = 0;
  for (const change of changes) {
    if (!change.diff) continue;
    for (const line of change.diff.split("\n")) {
      if (line.startsWith("+++") || line.startsWith("---")) continue;
      if (line.startsWith("+")) added++;
      else if (line.startsWith("-")) removed++;
    }
  }
  return { added, removed };
}

/**
 * 入参摘要：取 1-2 个有意义的字段，拼成 "k1: v1 · k2: v2"。
 * 用于通用 dynamic 兜底与 mcp 副标题。
 */
function summarizeArgs(args: unknown): string | undefined {
  if (!args || typeof args !== "object") return undefined;
  const obj = args as Record<string, unknown>;
  const keys = Object.keys(obj);
  if (keys.length === 0) return undefined;
  const parts: string[] = [];
  for (const key of keys.slice(0, 2)) {
    const value = obj[key];
    const formatted = formatArgValue(value);
    if (formatted == null) continue;
    parts.push(`${key}: ${formatted}`);
  }
  if (parts.length === 0) return undefined;
  let summary = parts.join(" · ");
  if (summary.length > 80) summary = summary.slice(0, 79) + "…";
  return summary;
}

function formatArgValue(value: unknown): string | null {
  if (value == null) return null;
  if (typeof value === "string") {
    if (value.length === 0) return null;
    return value.length > 40 ? value.slice(0, 39) + "…" : value;
  }
  if (typeof value === "number" || typeof value === "boolean") {
    return String(value);
  }
  if (Array.isArray(value)) {
    return `[${value.length}]`;
  }
  if (typeof value === "object") {
    return "{…}";
  }
  return null;
}

// ── 状态映射 ──

function getItemDisplayStatus(item: AgentDashThreadItem): DisplayStatus {
  switch (item.type) {
    case "commandExecution":
    case "fileChange":
    case "mcpToolCall":
    case "dynamicToolCall":
    case "collabAgentToolCall":
    case "fsRead":
    case "fsGrep":
    case "fsGlob":
      return item.status as DisplayStatus;
    default:
      return "completed";
  }
}

// ── 工具函数 ──

function str(
  args: Record<string, unknown> | null | undefined,
  key: string,
): string | null {
  if (!args) return null;
  const v = args[key];
  return typeof v === "string" && v.length > 0 ? v : null;
}

function num(
  args: Record<string, unknown> | null | undefined,
  key: string,
): number | null {
  if (!args) return null;
  const v = args[key];
  return typeof v === "number" && Number.isFinite(v) ? v : null;
}
