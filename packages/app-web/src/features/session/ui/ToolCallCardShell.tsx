/**
 * 工具调用卡片共享 shell
 *
 * 统一承载 header、状态指示、折叠、审批操作和错误展示。
 * 具体 renderer 只需返回 title + body，由 shell 包裹渲染。
 */

import { memo, useEffect, useRef, useState, type ReactNode } from "react";
import type { KindMeta } from "../model/threadItemKind";
import { approveToolCall, rejectToolCall } from "../../../services/executor";

export type DisplayStatus =
  | "inProgress"
  | "completed"
  | "failed"
  | "declined"
  | "pending";

const MIN_IN_PROGRESS_VISIBLE_MS = 600;

export interface ToolCallCardShellProps {
  kind: KindMeta;
  title: ReactNode;
  status: DisplayStatus;
  isPendingApproval?: boolean;
  sessionId?: string;
  itemId: string;
  durationMs?: number;
  defaultExpanded?: boolean;
  children: ReactNode;
}

export const ToolCallCardShell = memo(function ToolCallCardShell({
  kind,
  title,
  status,
  isPendingApproval,
  sessionId,
  itemId,
  durationMs,
  defaultExpanded,
  children,
}: ToolCallCardShellProps) {
  const shouldDefaultExpand =
    defaultExpanded ?? (Boolean(isPendingApproval) || status === "failed");
  const [expanded, setExpanded] = useState(shouldDefaultExpand);
  const [isSubmittingApproval, setIsSubmittingApproval] = useState(false);
  const [approvalError, setApprovalError] = useState<string | null>(null);
  const [renderStatus, setRenderStatus] = useState<DisplayStatus>(status);
  const inProgressSinceRef = useRef<number | null>(null);

  // 最小 inProgress 可见时间，避免闪烁
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
    if (isPendingApproval) setExpanded(true);
  }, [isPendingApproval]);

  const statusConfig = getStatusConfig(renderStatus, isPendingApproval);
  const elapsed = useElapsed(renderStatus === "inProgress");

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

  const displayDuration =
    durationMs != null && durationMs > 0
      ? formatDuration(durationMs)
      : elapsed;

  return (
    <div
      className={`rounded-[12px] border border-border bg-background transition-colors ${
        renderStatus === "failed" || renderStatus === "declined" ? "opacity-90" : ""
      }`}
    >
      {/* Header */}
      <button
        type="button"
        onClick={() => setExpanded(!expanded)}
        className="flex w-full items-center gap-2.5 px-3 py-2.5 text-left transition-colors hover:bg-secondary/35"
      >
        <span className="inline-flex shrink-0 rounded-[6px] border border-border bg-secondary px-1.5 py-0.5 text-[10px] font-semibold uppercase tracking-[0.1em] text-muted-foreground">
          {kind.badge}
        </span>

        <div className="min-w-0 flex-1">
          <p className="truncate text-sm font-medium text-foreground">{title}</p>
          <p className="text-xs text-muted-foreground">{kind.label}</p>
        </div>

        <div className="flex shrink-0 items-center gap-1.5">
          <span className={`inline-block h-1.5 w-1.5 rounded-full ${statusConfig.dot}`} />
          <span className={`text-xs ${statusConfig.color}`}>{statusConfig.label}</span>
          {displayDuration && (
            <span className="ml-1 tabular-nums text-[10px] text-muted-foreground/50">
              {displayDuration}
            </span>
          )}
        </div>

        <span className="shrink-0 text-[10px] text-muted-foreground/40">
          {expanded ? "▲" : "▼"}
        </span>
      </button>

      {/* Expanded body */}
      {expanded && (
        <div className="space-y-3 border-t border-border px-3 py-3">
          {isPendingApproval && (
            <div className="flex items-center gap-2 rounded-[8px] border border-border bg-secondary/40 px-2.5 py-2 text-sm text-muted-foreground">
              <span className="inline-flex rounded-[6px] border border-warning/25 bg-warning/10 px-1.5 py-0.5 text-[10px] font-semibold tracking-[0.1em] text-warning">
                审批
              </span>
              等待用户审批
            </div>
          )}

          {renderStatus === "declined" && (
            <div className="flex items-center gap-2 rounded-[8px] border border-border bg-secondary/40 px-2.5 py-2 text-sm text-muted-foreground">
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
                className="rounded-[8px] border border-success/30 bg-success/10 px-3 py-1.5 text-sm text-success transition-colors hover:bg-success/15 disabled:opacity-50"
              >
                {isSubmittingApproval ? "处理中…" : "批准"}
              </button>
              <button
                type="button"
                onClick={() => { void handleReject(); }}
                disabled={isSubmittingApproval}
                className="rounded-[8px] border border-warning/30 bg-warning/10 px-3 py-1.5 text-sm text-warning transition-colors hover:bg-warning/15 disabled:opacity-50"
              >
                拒绝
              </button>
            </div>
          )}

          {approvalError && (
            <div className="rounded-[8px] border border-destructive/30 bg-destructive/10 p-2 text-sm text-destructive">
              {approvalError}
            </div>
          )}

          {children}

          <p className="select-none font-mono text-[10px] text-muted-foreground/25">
            {itemId.slice(0, 8)}
          </p>
        </div>
      )}
    </div>
  );
});

// ── 内部工具函数 ──

function useElapsed(active: boolean): string | null {
  const [clock, setClock] = useState<{ start: number; now: number } | null>(null);

  useEffect(() => {
    if (!active) return;
    const start = Date.now();
    const update = () => setClock({ start, now: Date.now() });
    const firstTick = window.setTimeout(update, 0);
    const interval = window.setInterval(update, 1000);
    return () => {
      window.clearTimeout(firstTick);
      window.clearInterval(interval);
    };
  }, [active]);

  if (!active || clock === null) return null;

  const secs = Math.floor((clock.now - clock.start) / 1000);
  const m = Math.floor(secs / 60);
  const s = secs % 60;
  return `${m}:${String(s).padStart(2, "0")}`;
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

function formatDuration(ms: number): string {
  const secs = Math.floor(ms / 1000);
  if (secs < 60) return `${secs}s`;
  const m = Math.floor(secs / 60);
  const s = secs % 60;
  return `${m}:${String(s).padStart(2, "0")}`;
}
