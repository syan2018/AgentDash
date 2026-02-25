/**
 * ACP 工具调用卡片
 *
 * 显示工具调用的状态、输入和输出
 */

import { useState } from "react";
import type { SessionUpdate, ToolKind, ToolCallStatus, ToolCallContent } from "@agentclientprotocol/sdk";

export interface AcpToolCallCardProps {
  update: SessionUpdate;
  isPendingApproval?: boolean;
  compact?: boolean;
}

export function AcpToolCallCard({
  update,
  isPendingApproval,
  compact = false,
}: AcpToolCallCardProps) {
  const [expanded, setExpanded] = useState(isPendingApproval);

  const toolCallInfo = (() => {
    if (update.sessionUpdate === "tool_call") {
      return {
        toolCallId: update.toolCallId,
        title: update.title,
        kind: update.kind ?? ("other" as ToolKind),
        status: update.status ?? ("pending" as ToolCallStatus),
        content: update.content ?? [],
        rawInput: update.rawInput,
        rawOutput: update.rawOutput,
      };
    }
    if (update.sessionUpdate === "tool_call_update") {
      return {
        toolCallId: update.toolCallId,
        title: update.title ?? "工具调用",
        kind: (update.kind ?? "other") as ToolKind,
        status: (update.status ?? "pending") as ToolCallStatus,
        content: update.content ?? [],
        rawInput: update.rawInput,
        rawOutput: update.rawOutput,
      };
    }
    return null;
  })();

  if (!toolCallInfo) return null;

  const { toolCallId, title, kind, status, rawInput, rawOutput, content } =
    toolCallInfo;

  const statusConfig = getStatusConfig(status, isPendingApproval);
  const kindConfig = getKindConfig(kind);

  if (compact) {
    return (
      <div className="rounded border border-border bg-card px-2 py-1.5 text-xs">
        <div className="flex items-center gap-2">
          <span>{kindConfig.icon}</span>
          <span className="flex-1 truncate">{title}</span>
          <span className={statusConfig.color}>{statusConfig.label}</span>
        </div>
      </div>
    );
  }

  return (
    <div
      className={`rounded-md border border-border bg-card ${isPendingApproval ? "ring-2 ring-warning/30" : ""}`}
    >
      <button
        type="button"
        onClick={() => setExpanded(!expanded)}
        className="flex w-full items-center gap-2 px-3 py-2 text-left"
      >
        <span className="text-lg">{kindConfig.icon}</span>
        <div className="flex-1 min-w-0">
          <p className="text-sm font-medium text-foreground truncate">
            {title}
          </p>
          <p className="text-xs text-muted-foreground">
            {kindConfig.label} · {statusConfig.label}
          </p>
        </div>
        <span
          className={`text-xs ${statusConfig.color}`}
        >
          {expanded ? "收起" : "展开"}
        </span>
      </button>

      {expanded && (
        <div className="border-t border-border px-3 py-2 space-y-3">
          {isPendingApproval && (
            <div className="rounded bg-warning/10 p-2 text-sm text-warning">
              等待用户审批
            </div>
          )}

          {content && content.length > 0 && (
            <div>
              <p className="mb-1 text-xs text-muted-foreground">内容</p>
              <div className="space-y-2">
                {content.map((item: ToolCallContent, index: number) => (
                  <ContentBlockView key={index} content={item} />
                ))}
              </div>
            </div>
          )}

          {rawInput !== undefined && (
            <div>
              <p className="mb-1 text-xs text-muted-foreground">输入</p>
              <pre className="overflow-auto rounded-md bg-muted/50 p-2 text-xs">
                {safeJson(rawInput)}
              </pre>
            </div>
          )}

          {rawOutput !== undefined && (
            <div>
              <p className="mb-1 text-xs text-muted-foreground">输出</p>
              <pre className="overflow-auto rounded-md bg-muted/50 p-2 text-xs">
                {safeJson(rawOutput)}
              </pre>
            </div>
          )}

          <p className="text-xs text-muted-foreground/50">
            ID: {toolCallId}
          </p>
        </div>
      )}
    </div>
  );
}

function ContentBlockView({ content }: { content: ToolCallContent }) {
  if (content.type === "content") {
    const block = content.content;
    if (block.type === "text") {
      return (
        <div className="text-sm whitespace-pre-wrap">{block.text}</div>
      );
    }
    if (block.type === "image") {
      return (
        <img
          src={`data:${block.mimeType};base64,${block.data}`}
          alt=""
          className="max-h-48 rounded border border-border"
        />
      );
    }
    if (block.type === "resource_link") {
      return (
        <a
          href={block.uri}
          className="text-sm text-primary hover:underline"
          target="_blank"
          rel="noopener noreferrer"
        >
          {block.name}
        </a>
      );
    }
    return (
      <pre className="text-xs">{safeJson(block)}</pre>
    );
  }

  if (content.type === "diff") {
    return (
      <div className="rounded border border-border bg-muted/30 p-2">
        <p className="text-xs font-mono mb-1">{content.path}</p>
        {content.oldText && (
          <div className="text-xs text-destructive line-through">
            {content.oldText.slice(0, 100)}
            {content.oldText.length > 100 ? "..." : ""}
          </div>
        )}
        <div className="text-xs text-success">
          {content.newText.slice(0, 100)}
          {content.newText.length > 100 ? "..." : ""}
        </div>
      </div>
    );
  }

  if (content.type === "terminal") {
    return (
      <div className="rounded border border-border bg-muted/30 p-2">
        <p className="text-xs text-muted-foreground">
          终端: {content.terminalId}
        </p>
      </div>
    );
  }

  return null;
}

function getStatusConfig(
  status: ToolCallStatus,
  isPendingApproval?: boolean
): { label: string; color: string } {
  if (isPendingApproval) {
    return { label: "等待审批", color: "text-warning" };
  }

  switch (status) {
    case "pending":
      return { label: "等待中", color: "text-muted-foreground" };
    case "in_progress":
      return { label: "执行中", color: "text-primary animate-pulse" };
    case "completed":
      return { label: "已完成", color: "text-success" };
    case "failed":
      return { label: "失败", color: "text-destructive" };
    default:
      return { label: "未知", color: "text-muted-foreground" };
  }
}

function getKindConfig(kind: ToolKind): { label: string; icon: string } {
  switch (kind) {
    case "read":
      return { label: "读取", icon: "📖" };
    case "edit":
      return { label: "编辑", icon: "✏️" };
    case "delete":
      return { label: "删除", icon: "🗑️" };
    case "move":
      return { label: "移动", icon: "📦" };
    case "search":
      return { label: "搜索", icon: "🔍" };
    case "execute":
      return { label: "执行", icon: "⚡" };
    case "think":
      return { label: "思考", icon: "🧠" };
    case "fetch":
      return { label: "获取", icon: "🌐" };
    case "switch_mode":
      return { label: "切换模式", icon: "🔄" };
    case "other":
    default:
      return { label: "工具", icon: "🔧" };
  }
}

function safeJson(value: unknown): string {
  try {
    return JSON.stringify(value, null, 2);
  } catch {
    return String(value);
  }
}

export default AcpToolCallCard;
