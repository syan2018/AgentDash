/**
 * Task Agent 执行面板
 *
 * 设计理念：
 * - 流区域顶部注入 Task 上下文卡片（与流式输出中的 AcpTaskContextCard 视觉统一）
 * - 输入框预填充任务默认 prompt（仅首次），发送按钮显示"执行"
 * - 首次发送调用 startTaskExecution，后续直接 promptSession
 * - 状态/执行器信息由左侧 Task 详情面板承载，聊天区不重复
 */

import { useCallback, useEffect, useRef, useState, useMemo } from "react";
import { useNavigate } from "react-router-dom";
import { SessionChatView } from "../acp-session";
import { promptSession, type ExecutorConfig } from "../../services/executor";
import type { Artifact, SessionNavigationState, Task } from "../../types";
import { useStoryStore } from "../../stores/storyStore";

interface TaskAgentSessionPanelProps {
  task: Task;
  onTaskUpdated: (task: Task) => void;
}

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

/** 组装任务默认 prompt — 优先 prompt_template，回退到 description */
function buildDefaultPrompt(task: Task): string {
  const template = task.agent_binding?.prompt_template?.trim();
  if (template) return template;
  return task.description?.trim() ?? "";
}

// ─── 主组件 ────────────────────────────────────────────

export function TaskAgentSessionPanel({ task, onTaskUpdated }: TaskAgentSessionPanelProps) {
  const navigate = useNavigate();
  const startTaskExecution = useStoryStore((s) => s.startTaskExecution);
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
  const defaultPrompt = useMemo(() => buildDefaultPrompt(task), [task]);

  // ─── customSend ──────────────────────────────────────
  // 首次（无 session）→ startTaskExecution
  // 后续（有 session）→ 直接 promptSession，不走 task API

  const handleCustomSend = useCallback(async (
    sid: string | null,
    prompt: string,
    _execConfig?: ExecutorConfig,
  ) => {
    if (!sid) {
      const updated = await startTaskExecution(
        task.id,
        prompt ? { override_prompt: prompt } : undefined,
      );
      if (updated) onTaskUpdatedRef.current(updated);
      else throw new Error("启动执行失败");
    } else {
      if (!prompt) return;
      await promptSession(sid, { prompt });
    }
  }, [startTaskExecution, task.id]);

  const handleTurnEnd = useCallback(() => {
    void (async () => {
      const latest = await refreshTask(task.id);
      if (latest) onTaskUpdatedRef.current(latest);
    })();
  }, [refreshTask, task.id]);

  // ─── 取消执行 ─────────────────────────────────────────

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

  // ─── headerSlot：仅有 session 时显示 ─────────────────

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
      {cancelError && <span className="text-destructive">{cancelError}</span>}
    </div>
  ) : null;

  // ─── streamPrefixContent：与 AcpTaskContextCard 视觉统一的注入卡片 ──

  const contextSources = task.agent_binding?.context_sources ?? [];

  const streamPrefixContent = (
    <div className="rounded-[12px] border border-border bg-background overflow-hidden">
      <div className="flex items-center gap-2.5 px-3 py-2.5">
        <span className="inline-flex rounded-[6px] border border-border bg-secondary px-1.5 py-0.5 text-[10px] font-semibold uppercase tracking-[0.14em] text-muted-foreground">
          Task
        </span>
        <span className="text-sm font-medium text-foreground">{task.title}</span>
        {task.agent_binding?.agent_type && (
          <span className="rounded-[6px] border border-primary/20 bg-primary/10 px-1.5 py-0.5 text-[10px] text-primary">
            {task.agent_binding.agent_type}
          </span>
        )}
        <span className="ml-auto text-[10px] text-muted-foreground font-mono">
          {task.id.slice(0, 8)}…
        </span>
      </div>
      {(task.description || contextSources.length > 0) && (
        <div className="border-t border-border px-3 py-2.5 space-y-1.5">
          {task.description && (
            <p className="text-xs leading-relaxed text-foreground/80">{task.description}</p>
          )}
          {contextSources.length > 0 && (
            <p className="text-[11px] text-muted-foreground">
              {contextSources.length} 个上下文源已注入
            </p>
          )}
        </div>
      )}
    </div>
  );

  // ─── 渲染 ──────────────────────────────────────────────

  return (
    <SessionChatView
      sessionId={sessionId}
      showStatusBar={false}
      showExecutorSelector={false}
      headerSlot={headerSlot}
      streamPrefixContent={streamPrefixContent}
      customSend={handleCustomSend}
      onTurnEnd={handleTurnEnd}
      idleSendLabel={hasSession ? "发送" : "执行"}
      initialInputValue={hasSession ? undefined : defaultPrompt}
      inputPlaceholder={hasSession ? "输入追加指令，Ctrl+Enter 发送…" : "编辑执行指令或直接点击执行"}
    />
  );
}
