/**
 * Task Agent 执行面板
 *
 * 保留 Task 专有的执行控制（启动/继续/取消）和状态轮询，
 * 流式输出与聊天输入复用 SessionChatView。
 */

import { useCallback, useEffect, useRef, useState, useMemo } from "react";
import { useNavigate } from "react-router-dom";
import { SessionChatView } from "../acp-session";
import { TaskStatusBadge } from "../../components/ui/status-badge";
import type { Artifact, SessionNavigationState, Task } from "../../types";
import { useStoryStore } from "../../stores/storyStore";

interface TaskAgentSessionPanelProps {
  task: Task;
  onTaskUpdated: (task: Task) => void;
}

// ─── 工具执行统计 ──────────────────────────────────────

interface ToolExecutionStats {
  total: number;
  completed: number;
  failed: number;
  running: number;
}

function getArtifactStatus(artifact: Artifact): string | null {
  if (!artifact.content || typeof artifact.content !== "object" || Array.isArray(artifact.content)) {
    return null;
  }
  const status = (artifact.content as Record<string, unknown>).status;
  return typeof status === "string" ? status : null;
}

function collectToolExecutionStats(artifacts: Artifact[]): ToolExecutionStats {
  const stats: ToolExecutionStats = { total: 0, completed: 0, failed: 0, running: 0 };
  for (const artifact of artifacts) {
    if (artifact.artifact_type !== "tool_execution") continue;
    stats.total += 1;
    const status = getArtifactStatus(artifact);
    if (status === "completed") stats.completed += 1;
    else if (status === "failed" || status === "rejected" || status === "canceled") stats.failed += 1;
    else if (status === "in_progress") stats.running += 1;
  }
  return stats;
}

// ─── 主组件 ────────────────────────────────────────────

export function TaskAgentSessionPanel({ task, onTaskUpdated }: TaskAgentSessionPanelProps) {
  const navigate = useNavigate();
  const startTaskExecution = useStoryStore((s) => s.startTaskExecution);
  const continueTaskExecution = useStoryStore((s) => s.continueTaskExecution);
  const cancelTaskExecution = useStoryStore((s) => s.cancelTaskExecution);
  const refreshTask = useStoryStore((s) => s.refreshTask);
  const storeError = useStoryStore((s) => s.error);

  const [submitError, setSubmitError] = useState<string | null>(null);
  const [isSubmitting, setIsSubmitting] = useState(false);

  const hasSession = Boolean(task.session_id);
  const sessionId = task.session_id ?? null;
  const executorSessionId = task.executor_session_id ?? null;
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

  // Session 首次建立时刷新一次 Task 状态
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

  // 切换 Task 时重置错误
  useEffect(() => { setSubmitError(null); }, [task.id]);

  const toolStats = useMemo(() => collectToolExecutionStats(task.artifacts), [task.artifacts]);

  // ─── 执行控制 ──────────────────────────────────────────

  const handleStartExecution = useCallback(async () => {
    if (isSubmitting || executionLocked) return;
    setSubmitError(null);
    setIsSubmitting(true);
    try {
      const updated = await startTaskExecution(task.id);
      if (!updated) {
        setSubmitError(storeError ?? "启动执行失败，请稍后重试");
        return;
      }
      onTaskUpdated(updated);
    } catch (error) {
      setSubmitError(error instanceof Error ? error.message : "启动执行失败");
    } finally {
      setIsSubmitting(false);
    }
  }, [executionLocked, isSubmitting, onTaskUpdated, startTaskExecution, storeError, task.id]);

  const handleContinueExecution = useCallback(async (prompt?: string) => {
    if (isSubmitting || executionLocked) return;
    setSubmitError(null);
    setIsSubmitting(true);
    try {
      const updated = await continueTaskExecution(task.id, prompt ? { additional_prompt: prompt } : undefined);
      if (!updated) {
        setSubmitError(storeError ?? "继续执行失败，请稍后重试");
        return;
      }
      onTaskUpdated(updated);
    } catch (error) {
      setSubmitError(error instanceof Error ? error.message : "继续执行失败");
    } finally {
      setIsSubmitting(false);
    }
  }, [continueTaskExecution, executionLocked, isSubmitting, onTaskUpdated, storeError, task.id]);

  const handleCancel = useCallback(async () => {
    if (!hasSession || isSubmitting || !executionLocked) return;
    setSubmitError(null);
    setIsSubmitting(true);
    try {
      const updated = await cancelTaskExecution(task.id);
      if (updated) onTaskUpdated(updated);
      else setSubmitError("取消执行失败，请稍后重试");
    } catch (error) {
      setSubmitError(error instanceof Error ? error.message : "取消执行失败");
    } finally {
      setIsSubmitting(false);
    }
  }, [cancelTaskExecution, executionLocked, hasSession, isSubmitting, onTaskUpdated, task.id]);

  const handleRefresh = useCallback(async () => {
    setSubmitError(null);
    const latest = await refreshTask(task.id);
    if (!latest) {
      setSubmitError(storeError ?? "刷新状态失败");
      return;
    }
    onTaskUpdatedRef.current(latest);
  }, [refreshTask, storeError, task.id]);

  // Turn 结束时刷新 Task 状态
  const handleTurnEnd = useCallback(() => {
    void (async () => {
      const latest = await refreshTask(task.id);
      if (latest) onTaskUpdatedRef.current(latest);
    })();
  }, [refreshTask, task.id]);

  const navigateToSessionPage = useCallback(
    (targetSessionId: string) => {
      const state: SessionNavigationState = {
        task_context: { task_id: task.id, agent_binding: task.agent_binding },
        return_to: { owner_type: "task", story_id: task.story_id, task_id: task.id },
      };
      navigate(`/session/${targetSessionId}`, { state });
    },
    [navigate, task.agent_binding, task.id, task.story_id],
  );

  // ─── Task 控制栏 ──────────────────────────────────────

  const taskControlBar = (
    <div className="space-y-2">
      {/* 状态 + 统计 */}
      <div className="flex flex-wrap items-center gap-2 text-xs">
        <TaskStatusBadge status={task.status} />
        <span className="text-muted-foreground">
          工具 {toolStats.completed}/{toolStats.total}
          {toolStats.running > 0 && <span className="ml-1 text-primary">({toolStats.running} 运行中)</span>}
          {toolStats.failed > 0 && <span className="ml-1 text-destructive">({toolStats.failed} 失败)</span>}
        </span>
        <div className="ml-auto flex items-center gap-1.5 text-muted-foreground">
          <span>S: {sessionId ? `${sessionId.slice(0, 8)}…` : "-"}</span>
          <span>E: {executorSessionId ? `${executorSessionId.slice(0, 16)}…` : "-"}</span>
        </div>
      </div>

      {/* 操作按钮 */}
      <div className="flex flex-wrap items-center gap-1.5">
        {!hasSession && (
          <button
            type="button"
            disabled={isSubmitting}
            onClick={() => void handleStartExecution()}
            className="rounded-[10px] border border-primary bg-primary px-3 py-1.5 text-xs font-medium text-primary-foreground transition-colors hover:opacity-95 disabled:opacity-50"
          >
            {isSubmitting ? "启动中…" : "启动执行"}
          </button>
        )}
        {hasSession && !executionLocked && (
          <button
            type="button"
            disabled={isSubmitting}
            onClick={() => void handleContinueExecution()}
            className="rounded-[10px] border border-primary bg-primary px-3 py-1.5 text-xs font-medium text-primary-foreground transition-colors hover:opacity-95 disabled:opacity-50"
          >
            {isSubmitting ? "处理中…" : "继续执行"}
          </button>
        )}
        {hasSession && executionLocked && (
          <button
            type="button"
            disabled={isSubmitting}
            onClick={() => void handleCancel()}
            className="rounded-[10px] border border-border bg-background px-3 py-1.5 text-xs font-medium text-foreground transition-colors hover:bg-secondary disabled:opacity-50"
          >
            {isSubmitting ? "处理中…" : "取消执行"}
          </button>
        )}
        {hasSession && (
          <>
            <button
              type="button"
              disabled={isSubmitting}
              onClick={() => void handleRefresh()}
              className="rounded-[10px] border border-border bg-background px-2.5 py-1.5 text-xs text-foreground transition-colors hover:bg-secondary disabled:opacity-50"
            >
              刷新状态
            </button>
            <button
              type="button"
              onClick={() => navigateToSessionPage(sessionId!)}
              className="rounded-[10px] border border-border bg-background px-2.5 py-1.5 text-xs text-foreground transition-colors hover:bg-secondary"
            >
              会话页
            </button>
          </>
        )}
      </div>

      {submitError && (
        <div className="rounded-md border border-destructive/40 bg-destructive/10 px-3 py-2 text-xs text-destructive">
          {submitError}
        </div>
      )}
    </div>
  );

  // ─── 渲染 ──────────────────────────────────────────────

  return (
    <div className="flex h-full flex-col overflow-hidden">
      {/* Task 控制栏作为 inputPrefix 渲染在聊天输入上方 */}
      <div className="min-h-0 flex-1 overflow-hidden">
        <SessionChatView
          sessionId={sessionId}
          executorHint={task.agent_binding?.agent_type}
          inputPrefix={taskControlBar}
          onTurnEnd={handleTurnEnd}
        />
      </div>
    </div>
  );
}
