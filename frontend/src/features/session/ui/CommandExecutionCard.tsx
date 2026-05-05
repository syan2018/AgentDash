/**
 * 命令执行卡片 — 专用于 commandExecution 类型的 ThreadItem
 *
 * 与通用 AcpToolCallCard 分离，支持：
 * - 流式终端输出渲染（monospace + ANSI 序列转换）
 * - 进程元信息（command / cwd / exit code）
 * - 自动滚底 + 最大高度折叠
 * - Promote 到终端面板按钮（Phase 5）
 */

import { memo, useEffect, useRef, useState } from "react";
import type { ThreadItem } from "../../../generated/backbone-protocol";
import { getThreadItemStatus } from "../model/types";
import { useWorkspaceTabStore } from "../../../stores/workspaceTabStore";

type ExecStatus = "inProgress" | "completed" | "failed" | "pending";

export interface CommandExecutionCardProps {
  item: ThreadItem;
  /** 流式输出累积文本（由 command_output_delta 叠加） */
  outputText?: string;
  sessionId?: string;
}

export const CommandExecutionCard = memo(function CommandExecutionCard({
  item,
  outputText,
}: CommandExecutionCardProps) {
  const status = getThreadItemStatus(item) as ExecStatus;
  const outputRef = useRef<HTMLPreElement>(null);
  const [collapsed, setCollapsed] = useState(false);

  const command =
    item.type === "commandExecution" ? item.command : "(unknown)";
  const cwd =
    item.type === "commandExecution" ? item.cwd : undefined;
  const exitCode =
    item.type === "commandExecution" ? (item as Record<string, unknown>).exitCode as number | undefined : undefined;

  const elapsed = useElapsed(status === "inProgress");

  useEffect(() => {
    if (outputRef.current && status === "inProgress") {
      outputRef.current.scrollTop = outputRef.current.scrollHeight;
    }
  }, [outputText, status]);

  const statusConfig = getExecStatusConfig(status);

  const lineCount = outputText ? outputText.split("\n").length - 1 : 0;
  const shouldCollapse = lineCount > 80;
  const maxH = collapsed ? "max-h-16" : shouldCollapse ? "max-h-96" : "max-h-64";

  return (
    <div className="overflow-hidden rounded-[12px] border border-border bg-background">
      {/* Header */}
      <div className="flex items-center gap-2 px-3 py-2">
        <span className="inline-flex shrink-0 rounded-[6px] border border-border bg-secondary px-1.5 py-0.5 text-[10px] font-bold uppercase tracking-[0.1em] text-muted-foreground">
          RUN
        </span>
        <code className="min-w-0 flex-1 truncate font-mono text-sm text-foreground">
          $ {command}
        </code>
        <div className="flex shrink-0 items-center gap-1.5">
          <span className={`inline-block h-1.5 w-1.5 rounded-full ${statusConfig.dot}`} />
          <span className={`text-xs ${statusConfig.color}`}>
            {statusConfig.label}
          </span>
          {elapsed && (
            <span className="ml-1 tabular-nums text-[10px] text-muted-foreground/50">
              {elapsed}
            </span>
          )}
        </div>
      </div>

      {/* Sub-header: cwd */}
      {cwd && (
        <div className="border-t border-border/50 px-3 py-1 text-[11px] text-muted-foreground/60">
          cwd: <span className="font-mono">{cwd}</span>
        </div>
      )}

      {/* Output area */}
      {(outputText || status === "inProgress") && (
        <div className="relative border-t border-border">
          <pre
            ref={outputRef}
            className={`overflow-auto bg-zinc-950 px-3 py-2 font-mono text-xs leading-relaxed text-zinc-300 transition-[max-height] ${maxH}`}
          >
            {outputText || (
              <span className="animate-pulse text-muted-foreground/40">
                等待输出...
              </span>
            )}
            {status === "inProgress" && (
              <span className="inline-block h-3.5 w-1.5 animate-pulse bg-zinc-400/60" />
            )}
          </pre>

          {shouldCollapse && (
            <button
              type="button"
              onClick={() => setCollapsed(!collapsed)}
              className="absolute bottom-1 right-2 rounded bg-zinc-800/80 px-2 py-0.5 text-[10px] text-zinc-400 hover:bg-zinc-700/80"
            >
              {collapsed ? "展开" : "折叠"}
            </button>
          )}
        </div>
      )}

      {/* Footer */}
      <div className="flex items-center gap-2 border-t border-border px-3 py-1.5 text-xs text-muted-foreground">
        {exitCode !== undefined && (
          <span className={exitCode === 0 ? "text-success" : "text-destructive"}>
            exit: {exitCode}
          </span>
        )}
        {lineCount > 0 && (
          <span className="text-muted-foreground/50">{lineCount} 行</span>
        )}
        <button
          type="button"
          onClick={() => {
            useWorkspaceTabStore
              .getState()
              .openOrActivate("terminal", `terminal://${item.id}`);
          }}
          className="ml-auto rounded px-2 py-0.5 text-[10px] text-muted-foreground/70 transition-colors hover:bg-secondary hover:text-foreground"
        >
          在终端中查看
        </button>
      </div>
    </div>
  );
});

function useElapsed(active: boolean): string | null {
  const [, setTick] = useState(0);
  const startRef = useRef(Date.now());

  useEffect(() => {
    if (!active) return;
    startRef.current = Date.now();
    const id = setInterval(() => setTick((t) => t + 1), 1000);
    return () => clearInterval(id);
  }, [active]);

  if (!active) return null;

  const secs = Math.floor((Date.now() - startRef.current) / 1000);
  const m = Math.floor(secs / 60);
  const s = secs % 60;
  return `${m}:${String(s).padStart(2, "0")}`;
}

function getExecStatusConfig(status: ExecStatus): {
  label: string;
  color: string;
  dot: string;
} {
  switch (status) {
    case "pending":
      return { label: "等待中", color: "text-muted-foreground", dot: "bg-muted-foreground/50" };
    case "inProgress":
      return { label: "执行中", color: "text-primary", dot: "bg-primary animate-pulse" };
    case "completed":
      return { label: "完成", color: "text-success", dot: "bg-success" };
    case "failed":
      return { label: "失败", color: "text-destructive", dot: "bg-destructive" };
    default:
      return { label: "未知", color: "text-muted-foreground", dot: "bg-muted-foreground/50" };
  }
}

export default CommandExecutionCard;
