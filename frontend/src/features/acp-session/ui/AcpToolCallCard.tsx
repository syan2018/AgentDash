/**
 * ACP 工具调用卡片
 *
 * header 行复用与 EventStripCard / EventFullCard 一致的排版语言：
 *   [kind badge]  [title / subtitle]  [status dot · label]  [▲▼]
 *
 * 样式原则：badge 是唯一染色点，卡片外框保持 border-border。
 * 状态色仅影响 header 行右侧的 dot + label，不影响外框。
 */

import { useEffect, useRef, useState } from "react";
import type { SessionUpdate, ToolKind, ToolCallContent } from "@agentclientprotocol/sdk";
import { approveToolCall, rejectToolCall } from "../../../services/executor";

type ExtendedToolCallStatus =
  | "pending"
  | "in_progress"
  | "completed"
  | "failed"
  | "canceled"
  | "rejected";

const MIN_IN_PROGRESS_VISIBLE_MS = 600;

export interface AcpToolCallCardProps {
  update: SessionUpdate;
  isPendingApproval?: boolean;
  compact?: boolean;
  sessionId?: string;
}

export function AcpToolCallCard({
  update,
  isPendingApproval,
  compact = false,
  sessionId,
}: AcpToolCallCardProps) {
  const [expanded, setExpanded] = useState(isPendingApproval);
  const [isSubmittingApproval, setIsSubmittingApproval] = useState(false);
  const [approvalError, setApprovalError] = useState<string | null>(null);

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

  const { toolCallId, title, kind, status, rawInput, rawOutput, content } = toolCallInfo;
  const displayStatus = resolveDisplayStatus(status, rawOutput);
  const [renderStatus, setRenderStatus] = useState<ExtendedToolCallStatus>(displayStatus);
  const inProgressSinceRef = useRef<number | null>(null);

  useEffect(() => {
    const isRunning = displayStatus === "in_progress" || displayStatus === "pending";
    if (isRunning) {
      inProgressSinceRef.current = Date.now();
      setRenderStatus(displayStatus);
      return;
    }

    const startedAt = inProgressSinceRef.current;
    if (startedAt != null) {
      const elapsed = Date.now() - startedAt;
      const remain = MIN_IN_PROGRESS_VISIBLE_MS - elapsed;
      if (remain > 0) {
        const timer = setTimeout(() => {
          inProgressSinceRef.current = null;
          setRenderStatus(displayStatus);
        }, remain);
        return () => clearTimeout(timer);
      }
      inProgressSinceRef.current = null;
    }
    setRenderStatus(displayStatus);
  }, [displayStatus]);

  const statusConfig = getStatusConfig(renderStatus, isPendingApproval);
  const kindConfig = getKindConfig(kind);

  const handleApprove = async () => {
    if (!sessionId || isSubmittingApproval) return;
    setApprovalError(null);
    setIsSubmittingApproval(true);
    try {
      await approveToolCall(sessionId, toolCallId);
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
      await rejectToolCall(sessionId, toolCallId);
    } catch (error) {
      setApprovalError(error instanceof Error ? error.message : "拒绝失败");
    } finally {
      setIsSubmittingApproval(false);
    }
  };

  // ── compact 模式 ───────────────────────────────────────────────────────────
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

  // ── 完整卡片 ───────────────────────────────────────────────────────────────
  return (
    <div
      className={`rounded-[12px] border border-border bg-background transition-colors ${
        renderStatus === "failed" ||
        renderStatus === "canceled" ||
        renderStatus === "rejected"
          ? "opacity-90"
          : ""
      }`}
    >
      {/* header 行 */}
      <button
        type="button"
        onClick={() => setExpanded(!expanded)}
        className="flex w-full items-center gap-2.5 px-3 py-2.5 text-left hover:bg-secondary/35 transition-colors"
      >
        {/* kind badge — 中性，不染色 */}
        <span className="inline-flex shrink-0 rounded-[6px] border border-border bg-secondary px-1.5 py-0.5 text-[10px] font-semibold uppercase tracking-[0.1em] text-muted-foreground">
          {kindConfig.icon}
        </span>

        {/* title + subtitle */}
        <div className="min-w-0 flex-1">
          <p className="truncate text-sm font-medium text-foreground">{title}</p>
          <p className="text-xs text-muted-foreground">{kindConfig.label}</p>
        </div>

        {/* 状态：dot + label，是唯一的状态染色点 */}
        <div className="flex shrink-0 items-center gap-1.5">
          <span className={`inline-block h-1.5 w-1.5 rounded-full ${statusConfig.dot}`} />
          <span className={`text-xs ${statusConfig.color}`}>{statusConfig.label}</span>
        </div>

        <span className="shrink-0 text-[10px] text-muted-foreground/40">
          {expanded ? "▲" : "▼"}
        </span>
      </button>

      {/* 展开区 */}
      {expanded && (
        <div className="space-y-3 border-t border-border px-3 py-3">
          {/* 待审批提示 */}
          {isPendingApproval && (
            <div className="flex items-center gap-2 rounded-[10px] border border-border bg-secondary/40 px-2.5 py-2 text-sm text-muted-foreground">
              <span className="inline-flex rounded-[6px] border border-warning/25 bg-warning/10 px-1.5 py-0.5 text-[10px] font-semibold tracking-[0.1em] text-warning">
                审批
              </span>
              等待用户审批
            </div>
          )}

          {/* 取消/拒绝提示 */}
          {(displayStatus === "canceled" || displayStatus === "rejected") && (
            <div className="flex items-center gap-2 rounded-[10px] border border-border bg-secondary/40 px-2.5 py-2 text-sm text-muted-foreground">
              <span className="inline-flex rounded-[6px] border border-border bg-secondary px-1.5 py-0.5 text-[10px] font-semibold tracking-[0.1em] text-muted-foreground">
                {displayStatus === "canceled" ? "取消" : "拒绝"}
              </span>
              {displayStatus === "canceled" ? "已取消执行" : "已拒绝执行"}
            </div>
          )}

          {/* 审批按钮 */}
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

          {/* 审批错误 */}
          {approvalError && (
            <div className="rounded-[10px] border border-destructive/30 bg-destructive/10 p-2 text-sm text-destructive">
              {approvalError}
            </div>
          )}

          {/* 内容块 */}
          {content && content.length > 0 && (
            <div>
              <p className="mb-1.5 text-xs font-medium text-muted-foreground/60">内容</p>
              <div className="space-y-2">
                {content.map((item: ToolCallContent, index: number) => (
                  <ContentBlockView key={index} content={item} />
                ))}
              </div>
            </div>
          )}

          {/* 输入 */}
          {rawInput !== undefined && (
            <div>
              <p className="mb-1.5 text-xs font-medium text-muted-foreground/60">输入</p>
              <pre className="agentdash-chat-code-block">{safeJson(rawInput)}</pre>
            </div>
          )}

          {/* 输出 */}
          {rawOutput !== undefined && (
            <div>
              <p className="mb-1.5 text-xs font-medium text-muted-foreground/60">输出</p>
              <pre className="agentdash-chat-code-block max-h-64">{safeJson(rawOutput)}</pre>
            </div>
          )}

          {/* tool call ID */}
          <p className="select-none font-mono text-[10px] text-muted-foreground/25">
            {toolCallId.slice(0, 8)}
          </p>
        </div>
      )}
    </div>
  );
}

// ─── 辅助组件 ─────────────────────────────────────────────────────────────────

function ContentBlockView({ content }: { content: ToolCallContent }) {
  if (content.type === "content") {
    const block = content.content;
    if (block.type === "text") {
      return <div className="whitespace-pre-wrap text-sm leading-7">{block.text}</div>;
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
    return <pre className="font-mono text-xs">{safeJson(block)}</pre>;
  }

  if (content.type === "diff") {
    return (
      <div className="rounded-[10px] border border-border bg-secondary/70 p-2.5 font-mono text-xs">
        <p className="mb-1.5 text-muted-foreground">{content.path}</p>
        {content.oldText && (
          <div className="whitespace-pre-wrap text-destructive/70 line-through">
            {content.oldText.slice(0, 200)}
            {content.oldText.length > 200 ? "..." : ""}
          </div>
        )}
        <div className="whitespace-pre-wrap text-success">
          {content.newText.slice(0, 200)}
          {content.newText.length > 200 ? "..." : ""}
        </div>
      </div>
    );
  }

  if (content.type === "terminal") {
    return (
      <div className="rounded-[10px] border border-border bg-secondary/70 p-2.5">
        <p className="flex items-center gap-1.5 text-xs text-muted-foreground">
          <span className="inline-flex rounded-[6px] border border-border bg-background px-1.5 py-0.5 text-[10px] font-semibold tracking-[0.1em]">
            终端
          </span>
          {content.terminalId}
        </p>
      </div>
    );
  }

  return null;
}

// ─── 辅助函数 ─────────────────────────────────────────────────────────────────

function resolveDisplayStatus(
  status: ExtendedToolCallStatus,
  rawOutput: unknown,
): ExtendedToolCallStatus {
  if (
    rawOutput &&
    typeof rawOutput === "object" &&
    "approval_state" in rawOutput &&
    (rawOutput as { approval_state?: unknown }).approval_state === "rejected"
  ) {
    return "rejected";
  }
  return status;
}

function getStatusConfig(
  status: ExtendedToolCallStatus,
  isPendingApproval?: boolean,
): { label: string; color: string; dot: string } {
  if (isPendingApproval) {
    return { label: "等待审批", color: "text-warning", dot: "bg-warning animate-pulse" };
  }
  switch (status) {
    case "pending":
      return { label: "等待中", color: "text-muted-foreground", dot: "bg-muted-foreground/50" };
    case "in_progress":
      return { label: "执行中", color: "text-primary", dot: "bg-primary animate-pulse" };
    case "completed":
      return { label: "已完成", color: "text-success", dot: "bg-success" };
    case "failed":
      return { label: "失败", color: "text-destructive", dot: "bg-destructive" };
    case "canceled":
      return { label: "已取消", color: "text-muted-foreground", dot: "bg-muted-foreground/50" };
    case "rejected":
      return { label: "已拒绝", color: "text-warning", dot: "bg-warning" };
    default:
      return { label: "未知", color: "text-muted-foreground", dot: "bg-muted-foreground/50" };
  }
}

function getKindConfig(kind: ToolKind): { label: string; icon: string } {
  switch (kind) {
    case "read":        return { label: "读取", icon: "READ" };
    case "edit":        return { label: "编辑", icon: "EDIT" };
    case "delete":      return { label: "删除", icon: "DEL" };
    case "move":        return { label: "移动", icon: "MOVE" };
    case "search":      return { label: "搜索", icon: "FIND" };
    case "execute":     return { label: "执行", icon: "RUN" };
    case "think":       return { label: "思考", icon: "THNK" };
    case "fetch":       return { label: "获取", icon: "NET" };
    case "switch_mode": return { label: "切换模式", icon: "MODE" };
    case "other":
    default:            return { label: "工具", icon: "TOOL" };
  }
}

function safeJson(value: unknown): string {
  try { return JSON.stringify(value, null, 2); } catch { return String(value); }
}

export default AcpToolCallCard;
