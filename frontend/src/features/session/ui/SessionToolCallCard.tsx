/**
 * 工具调用卡片 — 基于 ThreadItem 渲染
 *
 * header 行排版：[kind badge]  [title / subtitle]  [status dot · label]  [▲▼]
 * badge 是唯一染色点，卡片外框保持 border-border。
 */

import { memo, useEffect, useRef, useState } from "react";
import type { ThreadItem } from "../../../generated/backbone-protocol";
import { getThreadItemTitle, getThreadItemStatus, getThreadItemKind } from "../model/types";
import { approveToolCall, rejectToolCall } from "../../../services/executor";

type DisplayStatus =
  | "inProgress"
  | "completed"
  | "failed"
  | "declined"
  | "pending";

const MIN_IN_PROGRESS_VISIBLE_MS = 600;

export interface AcpToolCallCardProps {
  item: ThreadItem;
  isPendingApproval?: boolean;
  compact?: boolean;
  sessionId?: string;
  outputText?: string;
}

export const AcpToolCallCard = memo(function AcpToolCallCard({
  item,
  isPendingApproval,
  compact = false,
  sessionId,
  outputText,
}: AcpToolCallCardProps) {
  const title = getThreadItemTitle(item);
  const status = getThreadItemStatus(item) as DisplayStatus;
  const kind = getThreadItemKind(item);
  const itemId = item.id;

  const [expanded, setExpanded] = useState(Boolean(isPendingApproval));
  const [isSubmittingApproval, setIsSubmittingApproval] = useState(false);
  const [approvalError, setApprovalError] = useState<string | null>(null);
  const [renderStatus, setRenderStatus] = useState<DisplayStatus>(status);
  const inProgressSinceRef = useRef<number | null>(null);

  useEffect(() => {
    const isRunning = status === "inProgress" || status === "pending";
    if (isRunning) {
      inProgressSinceRef.current = Date.now();
      setRenderStatus(status);
      return;
    }

    const startedAt = inProgressSinceRef.current;
    if (startedAt != null) {
      const elapsed = Date.now() - startedAt;
      const remain = MIN_IN_PROGRESS_VISIBLE_MS - elapsed;
      if (remain > 0) {
        const timer = setTimeout(() => {
          inProgressSinceRef.current = null;
          setRenderStatus(status);
        }, remain);
        return () => clearTimeout(timer);
      }
      inProgressSinceRef.current = null;
    }
    setRenderStatus(status);
  }, [status]);

  useEffect(() => {
    if (isPendingApproval) {
      setExpanded(true);
    }
  }, [isPendingApproval]);

  const statusConfig = getStatusConfig(renderStatus, isPendingApproval);
  const kindConfig = getKindConfig(kind);

  const handleApprove = async () => {
    if (!sessionId || isSubmittingApproval) return;
    setApprovalError(null);
    setIsSubmittingApproval(true);
    try {
      await approveToolCall(sessionId, itemId);
    } catch (error) {
      setApprovalError(error instanceof Error ? error.message : "审批失败");
    } finally {
      setIsSubmittingApproval(false);
    }
  };

  const handleReject = async () => {
    if (!sessionId || isSubmittingApproval) return;
    setApprovalError(null);
    setIsSubmittingApproval(true);
    try {
      await rejectToolCall(sessionId, itemId);
    } catch (error) {
      setApprovalError(error instanceof Error ? error.message : "拒绝失败");
    } finally {
      setIsSubmittingApproval(false);
    }
  };

  // ── compact 模式 ──
  if (compact) {
    return (
      <div className="rounded-[10px] border border-border bg-background px-2.5 py-2 text-xs">
        <div className="flex items-center gap-2">
          <span className="inline-flex shrink-0 rounded-[6px] border border-border bg-secondary px-1.5 py-0.5 text-[10px] font-semibold uppercase tracking-[0.1em] text-muted-foreground">
            {kindConfig.icon}
          </span>
          <span className="min-w-0 flex-1 truncate text-foreground/80">{title}</span>
          <span className={`shrink-0 text-[10px] ${statusConfig.color}`}>
            {statusConfig.label}
          </span>
        </div>
      </div>
    );
  }

  const detailContent = extractDetailContent(item);

  // ── 完整卡片 ──
  return (
    <div
      className={`rounded-[12px] border border-border bg-background transition-colors ${
        renderStatus === "failed" || renderStatus === "declined"
          ? "opacity-90"
          : ""
      }`}
    >
      <button
        type="button"
        onClick={() => setExpanded(!expanded)}
        className="flex w-full items-center gap-2.5 px-3 py-2.5 text-left hover:bg-secondary/35 transition-colors"
      >
        <span className="inline-flex shrink-0 rounded-[6px] border border-border bg-secondary px-1.5 py-0.5 text-[10px] font-semibold uppercase tracking-[0.1em] text-muted-foreground">
          {kindConfig.icon}
        </span>

        <div className="min-w-0 flex-1">
          <p className="truncate text-sm font-medium text-foreground">{title}</p>
          <p className="text-xs text-muted-foreground">{kindConfig.label}</p>
        </div>

        <div className="flex shrink-0 items-center gap-1.5">
          <span className={`inline-block h-1.5 w-1.5 rounded-full ${statusConfig.dot}`} />
          <span className={`text-xs ${statusConfig.color}`}>{statusConfig.label}</span>
        </div>

        <span className="shrink-0 text-[10px] text-muted-foreground/40">
          {expanded ? "▲" : "▼"}
        </span>
      </button>

      {expanded && (
        <div className="space-y-3 border-t border-border px-3 py-3">
          {isPendingApproval && (
            <div className="flex items-center gap-2 rounded-[10px] border border-border bg-secondary/40 px-2.5 py-2 text-sm text-muted-foreground">
              <span className="inline-flex rounded-[6px] border border-warning/25 bg-warning/10 px-1.5 py-0.5 text-[10px] font-semibold tracking-[0.1em] text-warning">
                审批
              </span>
              等待用户审批
            </div>
          )}

          {(renderStatus === "declined") && (
            <div className="flex items-center gap-2 rounded-[10px] border border-border bg-secondary/40 px-2.5 py-2 text-sm text-muted-foreground">
              <span className="inline-flex rounded-[6px] border border-border bg-secondary px-1.5 py-0.5 text-[10px] font-semibold tracking-[0.1em] text-muted-foreground">
                拒绝
              </span>
              已拒绝执行
            </div>
          )}

          {isPendingApproval && sessionId && (
            <div className="flex flex-wrap gap-2">
              <button
                type="button"
                onClick={() => { void handleApprove(); }}
                disabled={isSubmittingApproval}
                className="rounded-[10px] border border-success/30 bg-success/10 px-3 py-1.5 text-sm text-success transition-colors hover:bg-success/15 disabled:opacity-50"
              >
                {isSubmittingApproval ? "处理中…" : "批准"}
              </button>
              <button
                type="button"
                onClick={() => { void handleReject(); }}
                disabled={isSubmittingApproval}
                className="rounded-[10px] border border-warning/30 bg-warning/10 px-3 py-1.5 text-sm text-warning transition-colors hover:bg-warning/15 disabled:opacity-50"
              >
                拒绝
              </button>
            </div>
          )}

          {approvalError && (
            <div className="rounded-[10px] border border-destructive/30 bg-destructive/10 p-2 text-sm text-destructive">
              {approvalError}
            </div>
          )}

          {detailContent && (
            <div>
              <p className="mb-1.5 text-xs font-medium text-muted-foreground/60">{detailContent.label}</p>
              <pre className="agentdash-chat-code-block max-h-64">{detailContent.text}</pre>
            </div>
          )}

          {outputText && (
            <div>
              <p className="mb-1.5 text-xs font-medium text-muted-foreground/60">输出</p>
              <pre className="agentdash-chat-code-block max-h-64">{outputText}</pre>
            </div>
          )}

          <p className="select-none font-mono text-[10px] text-muted-foreground/25">
            {itemId.slice(0, 8)}
          </p>
        </div>
      )}
    </div>
  );
});

function extractDetailContent(item: ThreadItem): { label: string; text: string } | null {
  switch (item.type) {
    case "commandExecution": {
      if (item.aggregatedOutput) {
        return { label: "命令输出", text: item.aggregatedOutput };
      }
      return { label: "命令", text: `$ ${item.command}\n(cwd: ${item.cwd})` };
    }
    case "fileChange": {
      const diffs = item.changes.map((c) => `--- ${c.path}\n${c.diff}`).join("\n\n");
      return diffs ? { label: "文件变更", text: diffs } : null;
    }
    case "mcpToolCall": {
      const parts: string[] = [];
      if (item.arguments) parts.push(`输入: ${safeJson(item.arguments)}`);
      if (item.result) parts.push(`输出: ${safeJson(item.result)}`);
      if (item.error) parts.push(`错误: ${item.error.message}`);
      return parts.length > 0 ? { label: "MCP 工具", text: parts.join("\n\n") } : null;
    }
    case "dynamicToolCall": {
      const parts: string[] = [];
      if (item.arguments) parts.push(`输入: ${safeJson(item.arguments)}`);
      if (item.contentItems?.length) parts.push(`输出: ${safeJson(item.contentItems)}`);
      return parts.length > 0 ? { label: "工具调用", text: parts.join("\n\n") } : null;
    }
    default:
      return null;
  }
}

function getStatusConfig(
  status: DisplayStatus,
  isPendingApproval?: boolean,
): { label: string; color: string; dot: string } {
  if (isPendingApproval) {
    return { label: "等待审批", color: "text-warning", dot: "bg-warning animate-pulse" };
  }
  switch (status) {
    case "pending":
      return { label: "等待中", color: "text-muted-foreground", dot: "bg-muted-foreground/50" };
    case "inProgress":
      return { label: "执行中", color: "text-primary", dot: "bg-primary animate-pulse" };
    case "completed":
      return { label: "已完成", color: "text-success", dot: "bg-success" };
    case "failed":
      return { label: "失败", color: "text-destructive", dot: "bg-destructive" };
    case "declined":
      return { label: "已拒绝", color: "text-warning", dot: "bg-warning" };
    default:
      return { label: "未知", color: "text-muted-foreground", dot: "bg-muted-foreground/50" };
  }
}

function getKindConfig(kind: string): { label: string; icon: string } {
  switch (kind) {
    case "execute":    return { label: "执行", icon: "RUN" };
    case "edit":       return { label: "编辑", icon: "EDIT" };
    case "mcp":        return { label: "MCP", icon: "MCP" };
    case "tool":       return { label: "工具", icon: "TOOL" };
    case "search":     return { label: "搜索", icon: "FIND" };
    case "image":      return { label: "图片", icon: "IMG" };
    case "collab":     return { label: "协作", icon: "COLL" };
    default:           return { label: "工具", icon: "TOOL" };
  }
}

function safeJson(value: unknown): string {
  try { return JSON.stringify(value, null, 2); } catch { return String(value); }
}

export default AcpToolCallCard;
