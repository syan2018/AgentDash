/**
 * Task Agent 执行面板
 *
 * 设计理念：
 * - Task 已定义执行器/模型/工作空间，聊天区不重复展示
 * - 无 session 时展示任务上下文预览，用户点发送即启动执行
 * - 有 session 时展示实时流，可继续对话或取消
 * - 状态徽章、执行器信息由左侧 Task 详情面板承载
 */

import { useCallback, useEffect, useRef, useState, useMemo } from "react";
import { useNavigate } from "react-router-dom";
import { SessionChatView } from "../acp-session";
import type { ExecutorConfig } from "../../services/executor";
import type { Artifact, SessionNavigationState, Task } from "../../types";
import { useStoryStore } from "../../stores/storyStore";

interface TaskAgentSessionPanelProps {
  task: Task;
  onTaskUpdated: (task: Task) => void;
}

// ─── 工具执行统计 ──────────────────────────────────────

function countToolStats(artifacts: Artifact[]) {
  let total = 0, completed = 0, failed = 0, running = 0;
  for (const a of artifacts) {
    if (a.artifact_type !== "tool_execution") continue;
    total += 1;
    const s = a.content && typeof a.content === "object" && !Array.isArray(a.content)
      ? (a.content as Record<string, unknown>).status : null;
    if (s === "completed") completed += 1;
    else if (s === "failed" || s === "rejected" || s === "canceled") failed += 1;
    else if (s === "in_progress") running += 1;
  }
  return { total, completed, failed, running };
}

// ─── 主组件 ────────────────────────────────────────────

export function TaskAgentSessionPanel({ task, onTaskUpdated }: TaskAgentSessionPanelProps) {
  const navigate = useNavigate();
  const startTaskExecution = useStoryStore((s) => s.startTaskExecution);
  const continueTaskExecution = useStoryStore((s) => s.continueTaskExecution);
  const cancelTaskExecution = useStoryStore((s) => s.cancelTaskExecution);
  const refreshTask = useStoryStore((s) => s.refreshTask);

  const [cancelError, setCancelError] = useState<string | null>(null);

  const hasSession = Boolean(task.session_id);
  const sessionId = task.session_id ?? null;
  const executionLocked = task.status === "running";

  const onTaskUpdatedRef = useRef(onTaskUpdated);
  useEffect(() => { onTaskUpdatedRef.current = onTaskUpdated; }, [onTaskUpdated]);

  const taskSnapshotRef = useRef({ status: task.status, updated_at: task.updated_at });
  useEffect(() => {
    taskSnapshotRef.current = { status: task.status, updated_at: task.updated_at };
  }, [task.status, task.updated_at]);

  // 执行中轮询 Task 状态
  useEffect(() => {
    if (!hasSession || !executionLocked) return;
    const timer = window.setInterval(() => {
      void (async () => {
        const latest = await refreshTask(task.id);
        if (!latest) return;
        if (latest.status !== taskSnapshotRef.current.status || latest.updated_at !== taskSnapshotRef.current.updated_at) {
          onTaskUpdatedRef.current(latest);
        }
      })();
    }, 2000);
    return () => window.clearInterval(timer);
  }, [executionLocked, hasSession, refreshTask, task.id]);

  // Session 首次建立时刷新 Task 状态
  useEffect(() => {
    if (!hasSession) return;
    let cancelled = false;
    void (async () => {
      const latest = await refreshTask(task.id);
      if (cancelled || !latest) return;
      if (latest.status !== taskSnapshotRef.current.status || latest.updated_at !== taskSnapshotRef.current.updated_at) {
        onTaskUpdatedRef.current(latest);
      }
    })();
    return () => { cancelled = true; };
  }, [hasSession, refreshTask, task.id]);

  useEffect(() => { setCancelError(null); }, [task.id]);

  const toolStats = useMemo(() => countToolStats(task.artifacts), [task.artifacts]);

  // ─── customSend：全接管发送流程 ──────────────────────

  const handleCustomSend = useCallback(async (
    _sid: string | null,
    prompt: string,
    _execConfig?: ExecutorConfig,
  ) => {
    if (!hasSession) {
      const updated = await startTaskExecution(
        task.id,
        prompt ? { override_prompt: prompt } : undefined,
      );
      if (updated) onTaskUpdatedRef.current(updated);
      else throw new Error("启动执行失败");
    } else {
      const updated = await continueTaskExecution(
        task.id,
        prompt ? { additional_prompt: prompt } : undefined,
      );
      if (updated) onTaskUpdatedRef.current(updated);
      else throw new Error("继续执行失败");
    }
  }, [continueTaskExecution, hasSession, startTaskExecution, task.id]);

  const handleTurnEnd = useCallback(() => {
    void (async () => {
      const latest = await refreshTask(task.id);
      if (latest) onTaskUpdatedRef.current(latest);
    })();
  }, [refreshTask, task.id]);

  // ─── 取消执行（headerSlot 里的唯一按钮） ────────────

  const handleCancel = useCallback(async () => {
    if (!hasSession || !executionLocked) return;
    setCancelError(null);
    try {
      const updated = await cancelTaskExecution(task.id);
      if (updated) onTaskUpdatedRef.current(updated);
      else setCancelError("取消失败");
    } catch (e) {
      setCancelError(e instanceof Error ? e.message : "取消失败");
    }
  }, [cancelTaskExecution, executionLocked, hasSession, task.id]);

  const navigateToSessionPage = useCallback(() => {
    if (!sessionId) return;
    const state: SessionNavigationState = {
      task_context: { task_id: task.id, agent_binding: task.agent_binding },
      return_to: { owner_type: "task", story_id: task.story_id, task_id: task.id },
    };
    navigate(`/session/${sessionId}`, { state });
  }, [navigate, sessionId, task.agent_binding, task.id, task.story_id]);

  // ─── headerSlot：仅运行中显示紧凑控制栏 ─────────────

  const headerSlot = (hasSession && (executionLocked || toolStats.total > 0 || cancelError)) ? (
    <div className="flex shrink-0 items-center gap-2 border-b border-border bg-secondary/15 px-4 py-2 text-xs">
      {toolStats.total > 0 && (
        <span className="text-muted-foreground">
          工具 {toolStats.completed}/{toolStats.total}
          {toolStats.running > 0 && <span className="ml-1 text-primary">· {toolStats.running} 运行中</span>}
          {toolStats.failed > 0 && <span className="ml-1 text-destructive">· {toolStats.failed} 失败</span>}
        </span>
      )}
      <div className="ml-auto flex items-center gap-1.5">
        {executionLocked && (
          <button
            type="button"
            onClick={() => void handleCancel()}
            className="rounded-[8px] border border-destructive/40 bg-destructive/8 px-2.5 py-1 text-xs text-destructive transition-colors hover:bg-destructive/15"
          >
            取消任务
          </button>
        )}
        <button
          type="button"
          onClick={navigateToSessionPage}
          className="rounded-[8px] border border-border bg-background px-2 py-1 text-xs text-muted-foreground transition-colors hover:bg-secondary hover:text-foreground"
        >
          全屏
        </button>
      </div>
      {cancelError && (
        <span className="text-destructive">{cancelError}</span>
      )}
    </div>
  ) : null;

  // ─── emptyStateContent：任务上下文预览 ────────────────

  const contextSources = task.agent_binding?.context_sources ?? [];

  const emptyStateContent = (
    <div className="flex flex-1 flex-col items-center justify-center px-6 py-8">
      <div className="w-full max-w-lg space-y-4">
        {/* 任务描述卡片 */}
        <div className="rounded-[14px] border border-dashed border-primary/25 bg-primary/[0.03] px-5 py-4 space-y-2.5">
          <div className="flex items-center gap-2">
            <span className="rounded-full bg-primary/10 px-2.5 py-0.5 text-[11px] font-medium text-primary">
              Task
            </span>
            <span className="text-sm font-semibold text-foreground">{task.title}</span>
          </div>
          {task.description && (
            <p className="text-sm leading-relaxed text-muted-foreground">{task.description}</p>
          )}
          <div className="flex flex-wrap items-center gap-2 text-xs text-muted-foreground">
            {task.agent_binding?.agent_type && (
              <span className="rounded-full border border-border bg-background px-2 py-0.5">
                {task.agent_binding.agent_type}
              </span>
            )}
            {task.workspace_id && (
              <span className="rounded-full border border-border bg-background px-2 py-0.5">
                WS:{task.workspace_id.slice(0, 8)}
              </span>
            )}
            {contextSources.length > 0 && (
              <span className="rounded-full border border-border bg-background px-2 py-0.5">
                {contextSources.length} 个上下文
              </span>
            )}
          </div>
        </div>

        <p className="text-center text-xs text-muted-foreground/70">
          输入补充指令或直接发送以启动执行
        </p>
      </div>
    </div>
  );

  // ─── 渲染 ──────────────────────────────────────────────

  return (
    <SessionChatView
      sessionId={sessionId}
      showStatusBar={false}
      showExecutorSelector={false}
      headerSlot={headerSlot}
      emptyStateContent={emptyStateContent}
      customSend={handleCustomSend}
      onTurnEnd={handleTurnEnd}
      inputPlaceholder={hasSession ? "输入追加指令，Ctrl+Enter 发送…" : "补充执行指令（可选），Ctrl+Enter 启动…"}
    />
  );
}
