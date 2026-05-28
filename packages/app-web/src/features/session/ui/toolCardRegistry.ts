/**
 * ThreadItem → renderer 一级分发注册表
 *
 * 根据 ThreadItem.type 选择对应的 renderer，返回 { kind, title, body, status }，
 * 由 ToolCallCardShell 包裹渲染。
 *
 * dynamicToolCall 内部按 tool 名做二级摘要。
 */

import { createElement, type ReactNode } from "react";
import type { ThreadItem, AgentDashThreadItem } from "../../../generated/backbone-protocol";
import { resolveKind, type KindMeta } from "../model/threadItemKind";
import type { DisplayStatus } from "./ToolCallCardShell";

import { CommandExecutionCardBody } from "./bodies/CommandExecutionCardBody";
import { FileChangeCardBody } from "./bodies/FileChangeCardBody";
import { McpCardBody } from "./bodies/McpCardBody";
import { WebSearchCardBody } from "./bodies/WebSearchCardBody";
import { ImageCardBody } from "./bodies/ImageCardBody";
import { CollabAgentCardBody } from "./bodies/CollabAgentCardBody";
import { ContextCompactionCardBody } from "./bodies/ContextCompactionCardBody";
import { DynamicToolCallCardBody } from "./bodies/DynamicToolCallCardBody";
import { GenericJsonBody } from "./bodies/GenericJsonBody";

export interface CardContext {
  sessionId?: string;
  outputText?: string;
}

export interface CardRenderResult {
  kind: KindMeta;
  title: ReactNode;
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
        title: createElement(
          "code",
          { className: "font-mono" },
          `$ ${item.command}`,
        ),
        body: createElement(CommandExecutionCardBody, {
          item,
          outputText: ctx.outputText,
          sessionId: ctx.sessionId,
        }),
        status,
        durationMs: item.durationMs ?? undefined,
      };

    case "fileChange":
      return {
        kind,
        title: fileChangeTitle(item),
        body: createElement(FileChangeCardBody, { item }),
        status,
      };

    case "mcpToolCall":
      return {
        kind,
        title: `${item.server}/${item.tool}`,
        body: createElement(McpCardBody, { item }),
        status,
        durationMs: item.durationMs ?? undefined,
      };

    case "webSearch":
      return {
        kind,
        title: `Search "${truncate(item.query, 80)}"`,
        body: createElement(WebSearchCardBody, { item }),
        status,
      };

    case "imageView":
      return {
        kind,
        title: `View ${truncate(item.path, 80)}`,
        body: createElement(ImageCardBody, { item }),
        status,
      };

    case "imageGeneration":
      return {
        kind,
        title: "Generate image",
        body: createElement(ImageCardBody, { item }),
        status,
      };

    case "collabAgentToolCall":
      return {
        kind,
        title: `${item.tool} agent`,
        body: createElement(CollabAgentCardBody, { item }),
        status,
      };

    case "contextCompaction":
      return {
        kind,
        title: "上下文压缩",
        body: createElement(ContextCompactionCardBody),
        status: status === "inProgress" ? "inProgress" : "completed",
      };

    case "dynamicToolCall":
      return {
        kind,
        title: getDynamicToolTitle(item),
        body: createElement(DynamicToolCallCardBody, { item }),
        status,
        durationMs: item.durationMs ?? undefined,
      };

    // AgentDash native items
    case "fsRead":
      return {
        kind,
        title: fsReadTitle(item),
        body: createElement(GenericJsonBody, {
          arguments: item.arguments,
          contentItems: item.contentItems,
        }),
        status,
      };

    case "fsGrep":
      return {
        kind,
        title: fsGrepTitle(item),
        body: createElement(GenericJsonBody, {
          arguments: item.arguments,
          contentItems: item.contentItems,
        }),
        status,
      };

    case "fsGlob":
      return {
        kind,
        title: `Glob ${truncate(item.pattern, 60)}`,
        body: createElement(GenericJsonBody, {
          arguments: item.arguments,
          contentItems: item.contentItems,
        }),
        status,
      };

    default:
      return { kind, title: "未知", body: null, status };
  }
}

// ── dynamicToolCall 二级摘要 ──

type DynamicItem = Extract<ThreadItem, { type: "dynamicToolCall" }>;

function getDynamicToolTitle(item: DynamicItem): string {
  const args = item.arguments as Record<string, unknown> | null;
  const tool = item.tool.toLowerCase();

  switch (tool) {
    case "read": {
      const path = str(args, "path");
      const offset = num(args, "offset");
      const limit = num(args, "limit");
      if (!path) return "Read";
      let label = `Read ${truncate(path, 60)}`;
      if (offset != null && limit != null) label += `:${offset}–${offset + limit}`;
      return label;
    }
    case "write": {
      const path = str(args, "file_path") ?? str(args, "path");
      return path ? `Write ${truncate(path, 60)}` : "Write";
    }
    case "edit":
    case "str_replace_editor":
    case "applypatch": {
      const path = str(args, "path") ?? str(args, "file_path");
      return path ? `Edit ${truncate(path, 60)}` : "Edit";
    }
    case "grep": {
      const pattern = str(args, "pattern");
      const path = str(args, "path") ?? str(args, "glob");
      let label = pattern ? `Grep "${truncate(pattern, 40)}"` : "Grep";
      if (path) label += ` in ${truncate(path, 30)}`;
      return label;
    }
    case "glob": {
      const pattern = str(args, "pattern") ?? str(args, "glob_pattern");
      return pattern ? `Glob ${truncate(pattern, 60)}` : "Glob";
    }
    case "websearch": {
      const query = str(args, "search_term") ?? str(args, "query");
      return query ? `Search "${truncate(query, 60)}"` : "WebSearch";
    }
    case "webfetch":
    case "fetch": {
      const url = str(args, "url");
      return url ? `Fetch ${truncate(url, 60)}` : "WebFetch";
    }
    case "todowrite": {
      const todos = args?.todos;
      const count = Array.isArray(todos) ? todos.length : 0;
      return count > 0 ? `更新 ${count} 项 todo` : "TodoWrite";
    }
    case "askquestion":
    case "askuserquestion": {
      const questions = args?.questions;
      const first =
        Array.isArray(questions) && questions[0]
          ? ((questions[0] as Record<string, unknown>).prompt ??
            (questions[0] as Record<string, unknown>).question)
          : null;
      const q = typeof first === "string" ? truncate(first, 50) : "";
      const n = Array.isArray(questions) ? questions.length : 0;
      let label = q ? `提问 ${q}` : "AskQuestion";
      if (n > 1) label += ` (+${n - 1})`;
      return label;
    }
    default: {
      const ns = item.namespace;
      return ns ? `${ns}/${item.tool}` : item.tool;
    }
  }
}

// ── fileChange 标题 ──

function fileChangeTitle(
  item: Extract<ThreadItem, { type: "fileChange" }>,
): string {
  const n = item.changes.length;
  if (n === 0) return "文件变更";
  if (n === 1) return item.changes[0]!.path;
  return `${item.changes[0]!.path} (+${n - 1} files)`;
}

// ── AgentDash native item 标题 ──

type FsReadItem = Extract<AgentDashThreadItem, { type: "fsRead" }>;
type FsGrepItem = Extract<AgentDashThreadItem, { type: "fsGrep" }>;

function fsReadTitle(item: FsReadItem): string {
  let label = `Read ${truncate(item.path, 60)}`;
  if (item.offset != null && item.limit != null) {
    label += `:${item.offset}–${item.offset + item.limit}`;
  }
  return label;
}

function fsGrepTitle(item: FsGrepItem): string {
  let label = `Grep "${truncate(item.pattern, 40)}"`;
  const target = item.path ?? item.glob;
  if (target) label += ` in ${truncate(target, 30)}`;
  return label;
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

function truncate(s: string, max: number): string {
  return s.length > max ? s.slice(0, max) + "…" : s;
}
