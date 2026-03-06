/**
 * ACP 工具调用卡片
 *
 * 显示工具调用的状态、输入和输出。
 * 对照 Zed 实现完整支持所有 ToolCallStatus：
 * pending / in_progress / completed / failed / canceled / rejected
 */

import { useState } from "react";
import type { SessionUpdate, ToolKind, ToolCallContent } from "@agentclientprotocol/sdk";

/**
 * 扩展的工具调用状态：SDK 标准 + Zed 扩展（canceled/rejected）。
 * SDK v0.14 只定义了 pending/in_progress/completed/failed，
 * 但后端可能发送扩展状态。
 */
type ExtendedToolCallStatus = "pending" | "in_progress" | "completed" | "failed" | "canceled" | "rejected";

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
        status: (update.status ?? "pending") as ExtendedToolCallStatus,
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
        status: (update.status ?? "pending") as ExtendedToolCallStatus,
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
      <div className="rounded-[10px] border border-border bg-background px-2.5 py-2 text-xs">
        <div className="flex items-center gap-2">
          <span className="inline-flex rounded-[6px] border border-border bg-secondary px-1.5 py-0.5 text-[10px] font-semibold uppercase tracking-[0.14em] text-muted-foreground">{kindConfig.icon}</span>
          <span className="flex-1 truncate">{title}</span>
          <span className={statusConfig.color}>{statusConfig.label}</span>
        </div>
      </div>
    );
  }

  return (
    <div
      className={`rounded-[12px] border ${statusConfig.borderColor} bg-background transition-colors ${
        isPendingApproval
          ? "ring-2 ring-warning/20"
          : status === "failed" || status === "canceled" || status === "rejected"
            ? "opacity-90"
            : ""
      }`}
    >
      <button
        type="button"
        onClick={() => setExpanded(!expanded)}
        className="flex w-full items-center gap-2.5 px-3 py-2.5 text-left"
      >
        <span className="inline-flex min-w-10 shrink-0 items-center justify-center rounded-[8px] border border-border bg-secondary px-2 py-1 text-[10px] font-semibold uppercase tracking-[0.14em] text-muted-foreground">
          {kindConfig.icon}
        </span>
        <div className="flex-1 min-w-0">
          <p className="text-sm font-medium text-foreground truncate">
            {title}
          </p>
          <p className="text-xs text-muted-foreground">
            {kindConfig.label}
          </p>
        </div>
        <div className="flex items-center gap-1.5">
          <span className={`inline-block h-2 w-2 rounded-full ${statusConfig.dot}`} />
          <span className={`text-xs ${statusConfig.color}`}>
            {statusConfig.label}
          </span>
        </div>
        <span className="text-xs text-muted-foreground/50">
          {expanded ? "▲" : "▼"}
        </span>
      </button>

      {expanded && (
        <div className="space-y-3 border-t border-border px-3 py-3">
          {isPendingApproval && (
            <div className="flex items-center gap-2 rounded-[10px] border border-warning/20 bg-warning/10 p-2.5 text-sm text-warning">
              <span className="inline-flex rounded-[6px] border border-warning/30 bg-background px-1.5 py-0.5 text-[10px] font-semibold uppercase tracking-[0.14em]">WAIT</span>
              <span>等待用户审批</span>
            </div>
          )}

          {(status === "canceled" || status === "rejected") && (
            <div className="flex items-center gap-2 rounded-[10px] border border-border bg-secondary/70 p-2.5 text-sm text-muted-foreground">
              <span className="inline-flex rounded-[6px] border border-border bg-background px-1.5 py-0.5 text-[10px] font-semibold uppercase tracking-[0.14em]">
                {status === "canceled" ? "STOP" : "NO"}
              </span>
              <span>{status === "canceled" ? "已取消执行" : "已拒绝执行"}</span>
            </div>
          )}

          {content && content.length > 0 && (
            <div>
              <p className="mb-1.5 text-xs font-medium text-muted-foreground">内容</p>
              <div className="space-y-2">
                {content.map((item: ToolCallContent, index: number) => (
                  <ContentBlockView key={index} content={item} />
                ))}
              </div>
            </div>
          )}

          {rawInput !== undefined && (
            <div>
              <p className="mb-1.5 text-xs font-medium text-muted-foreground">输入</p>
              <pre className="agentdash-chat-code-block">
                {safeJson(rawInput)}
              </pre>
            </div>
          )}

          {rawOutput !== undefined && (
            <div>
              <p className="mb-1.5 text-xs font-medium text-muted-foreground">输出</p>
              <pre className="agentdash-chat-code-block max-h-64">
                {safeJson(rawOutput)}
              </pre>
            </div>
          )}

          <p className="text-xs text-muted-foreground/40 font-mono">
            {toolCallId}
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
        <div className="whitespace-pre-wrap text-sm leading-7">{block.text}</div>
      );
    }
    if (block.type === "image") {
      return (
        <img
          src={`data:${block.mimeType};base64,${block.data}`}
          alt=""
          className="max-h-48 rounded-[10px] border border-border"
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
      <pre className="text-xs font-mono">{safeJson(block)}</pre>
    );
  }

  if (content.type === "diff") {
    return (
      <div className="rounded-[10px] border border-border bg-secondary/70 p-2.5 font-mono text-xs">
        <p className="mb-1.5 text-muted-foreground">{content.path}</p>
        {content.oldText && (
          <div className="text-destructive/70 line-through whitespace-pre-wrap">
            {content.oldText.slice(0, 200)}
            {content.oldText.length > 200 ? "..." : ""}
          </div>
        )}
        <div className="text-success whitespace-pre-wrap">
          {content.newText.slice(0, 200)}
          {content.newText.length > 200 ? "..." : ""}
        </div>
      </div>
    );
  }

  if (content.type === "terminal") {
    return (
      <div className="rounded-[10px] border border-border bg-secondary/70 p-2.5">
        <p className="text-xs text-muted-foreground flex items-center gap-1.5">
          <span className="inline-flex rounded-[6px] border border-border bg-background px-1.5 py-0.5 text-[10px] font-semibold uppercase tracking-[0.14em]">TERM</span>
          终端: {content.terminalId}
        </p>
      </div>
    );
  }

  return null;
}

function getStatusConfig(
  status: ExtendedToolCallStatus,
  isPendingApproval?: boolean
): { label: string; color: string; dot: string; borderColor: string } {
  if (isPendingApproval) {
    return { label: "等待审批", color: "text-warning", dot: "bg-warning animate-pulse", borderColor: "border-warning/30" };
  }

  switch (status) {
    case "pending":
      return { label: "等待中", color: "text-muted-foreground", dot: "bg-muted-foreground", borderColor: "border-border" };
    case "in_progress":
      return { label: "执行中", color: "text-primary", dot: "bg-primary animate-pulse", borderColor: "border-primary/30" };
    case "completed":
      return { label: "已完成", color: "text-success", dot: "bg-success", borderColor: "border-success/30" };
    case "failed":
      return { label: "失败", color: "text-destructive", dot: "bg-destructive", borderColor: "border-destructive/30" };
    case "canceled":
      return { label: "已取消", color: "text-muted-foreground", dot: "bg-muted-foreground", borderColor: "border-border" };
    case "rejected":
      return { label: "已拒绝", color: "text-warning", dot: "bg-warning", borderColor: "border-warning/30" };
    default:
      return { label: "未知", color: "text-muted-foreground", dot: "bg-muted-foreground", borderColor: "border-border" };
  }
}

function getKindConfig(kind: ToolKind): { label: string; icon: string } {
  switch (kind) {
    case "read":
      return { label: "读取", icon: "READ" };
    case "edit":
      return { label: "编辑", icon: "EDIT" };
    case "delete":
      return { label: "删除", icon: "DEL" };
    case "move":
      return { label: "移动", icon: "MOVE" };
    case "search":
      return { label: "搜索", icon: "FIND" };
    case "execute":
      return { label: "执行", icon: "RUN" };
    case "think":
      return { label: "思考", icon: "THNK" };
    case "fetch":
      return { label: "获取", icon: "NET" };
    case "switch_mode":
      return { label: "切换模式", icon: "MODE" };
    case "other":
    default:
      return { label: "工具", icon: "TOOL" };
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
