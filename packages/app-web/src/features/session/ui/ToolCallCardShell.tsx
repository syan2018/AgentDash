/**
 * 工具调用卡片共享 shell
 *
 * 统一承载 header、状态指示、折叠、审批操作和错误展示。
 * 具体 renderer 只需返回 title + body，由 shell 包裹渲染。
 */

import { memo, useEffect, useRef, useState, type ReactNode } from "react";
import type { KindMeta } from "../model/threadItemKind";
import {
  approveToolCall,
  approveToolCallForAgentRun,
  rejectToolCall,
  rejectToolCallForAgentRun,
} from "../../../services/executor";
import type { AgentRunRuntimeTarget } from "../../../services/agentRunRuntime";
import type { ToolCardHeaderModel } from "./ToolCardHeader";
import { ST } from "./bodies/cardBodyTokens";

export type DisplayStatus =
  | "inProgress"
  | "completed"
  | "failed"
  | "declined"
  | "pending";

const MIN_IN_PROGRESS_VISIBLE_MS = 600;

export interface ToolCallCardShellProps {
  kind: KindMeta;
  header: ToolCardHeaderModel;
  status: DisplayStatus;
  isPendingApproval?: boolean;
  agentRunTarget?: AgentRunRuntimeTarget | null;
  sessionId?: string;
  itemId: string;
  durationMs?: number;
  defaultExpanded?: boolean;
  children: ReactNode;
}

export const ToolCallCardShell = memo(function ToolCallCardShell({
  kind,
  header,
  status,
  isPendingApproval,
  agentRunTarget,
  sessionId,
  itemId,
  durationMs,
  defaultExpanded,
  children,
}: ToolCallCardShellProps) {
  const needsAttention = Boolean(isPendingApproval) || status === "failed" || status === "declined";

  const shouldDefaultExpand =
    defaultExpanded ?? needsAttention;
  const [expanded, setExpanded] = useState(shouldDefaultExpand);
  const [isSubmittingApproval, setIsSubmittingApproval] = useState(false);
  const [approvalError, setApprovalError] = useState<string | null>(null);
  const [renderStatus, setRenderStatus] = useState<DisplayStatus>(status);
  const inProgressSinceRef = useRef<number | null>(null);

  useEffect(() => {
    const running = status === "inProgress" || status === "pending";
    if (running) {
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
    if ((!agentRunTarget && !sessionId) || isSubmittingApproval) return;
    setApprovalError(null);
    setIsSubmittingApproval(true);
    try {
      if (agentRunTarget) {
        await approveToolCallForAgentRun(agentRunTarget, itemId);
      } else if (sessionId) {
        await approveToolCall(sessionId, itemId);
      }
    } catch (error) {
      setApprovalError(error instanceof Error ? error.message : "审批失败");
    } finally {
      setIsSubmittingApproval(false);
    }
  };

  const handleReject = async () => {
    if ((!agentRunTarget && !sessionId) || isSubmittingApproval) return;
    setApprovalError(null);
    setIsSubmittingApproval(true);
    try {
      if (agentRunTarget) {
        await rejectToolCallForAgentRun(agentRunTarget, itemId);
      } else if (sessionId) {
        await rejectToolCall(sessionId, itemId);
      }
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

  // ── 统一渲染：标题栏 + body ──
  // 所有状态共享同一结构：标题栏（可点击折叠）+ 展开后 body
  // 状态差异仅通过标题栏背景色和状态指示器体现

  const headerBg =
    renderStatus === "failed" || renderStatus === "declined"
      ? "bg-destructive/5"
      : isPendingApproval
        ? "bg-warning/5"
        : renderStatus === "inProgress" || renderStatus === "pending"
          ? "bg-primary/5"
          : expanded
            ? "bg-secondary/30"
            : "";

  const statusLabel = needsAttention || renderStatus === "inProgress" || renderStatus === "pending";

  return (
    <div>
      <button
        type="button"
        onClick={() => setExpanded(!expanded)}
        className={`${ST.itemRow} ${headerBg}`}
      >
        <span className={`${ST.dot} ${statusConfig.dot}`} />
        <span className={ST.badge}>
          {kind.badge}
        </span>
        <span className={ST.title}>
          {header.primary}
        </span>
        {statusLabel && (
          <span className={`shrink-0 text-[10px] ${statusConfig.color}`}>{statusConfig.label}</span>
        )}
        {displayDuration && (
          <span className="shrink-0 tabular-nums text-[10px] text-muted-foreground/40">
            {displayDuration}
          </span>
        )}
      </button>

      {expanded && (
        <div className={ST.bodyArea}>
          {header.secondary != null && header.secondary !== "" && (
            <p className="text-[10px] text-muted-foreground/50">{header.secondary}</p>
          )}

          {isPendingApproval && (
            <div className="flex items-center gap-2 text-xs text-warning">
              <span className="inline-flex rounded-[4px] border border-warning/25 bg-warning/10 px-1 py-px text-[9px] font-semibold tracking-[0.08em]">
                审批
              </span>
              等待用户审批
            </div>
          )}

          {renderStatus === "declined" && (
            <div className="flex items-center gap-2 text-xs text-muted-foreground">
              <span className="inline-flex rounded-[4px] border border-border bg-secondary px-1 py-px text-[9px] font-semibold tracking-[0.08em]">
                拒绝
              </span>
              已拒绝执行
            </div>
          )}

          {isPendingApproval && (agentRunTarget || sessionId) && (
            <div className="flex flex-wrap gap-2">
              <button
                type="button"
                onClick={() => { void handleApprove(); }}
                disabled={isSubmittingApproval}
                className="rounded-[6px] border border-success/30 bg-success/10 px-2.5 py-1 text-xs text-success transition-colors hover:bg-success/15 disabled:opacity-50"
              >
                {isSubmittingApproval ? "处理中…" : "批准"}
              </button>
              <button
                type="button"
                onClick={() => { void handleReject(); }}
                disabled={isSubmittingApproval}
                className="rounded-[6px] border border-warning/30 bg-warning/10 px-2.5 py-1 text-xs text-warning transition-colors hover:bg-warning/15 disabled:opacity-50"
              >
                拒绝
              </button>
            </div>
          )}

          {approvalError && (
            <div className="rounded-[6px] bg-destructive/5 px-2 py-1.5 text-xs text-destructive">
              {approvalError}
            </div>
          )}

          {children}
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
