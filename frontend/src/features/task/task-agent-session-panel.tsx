import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useNavigate } from "react-router-dom";
import { AcpSessionEntry, useAcpSession } from "../acp-session";
import { isAggregatedGroup, isAggregatedThinkingGroup } from "../acp-session/model/types";
import type { AcpDisplayItem, TokenUsageInfo } from "../acp-session/model/types";
import { extractAgentDashMetaFromUpdate } from "../acp-session/model/agentdashMeta";
import { TaskStatusBadge } from "../../components/ui/status-badge";
import type { Artifact, SessionNavigationState, Task } from "../../types";
import { useStoryStore } from "../../stores/storyStore";

interface TaskAgentSessionPanelProps {
  task: Task;
  onTaskUpdated: (task: Task) => void;
}

interface ToolExecutionStats {
  total: number;
  completed: number;
  failed: number;
  running: number;
  pending: number;
}

function getItemKey(item: AcpDisplayItem): string {
  if (isAggregatedGroup(item)) return item.groupKey;
  if (isAggregatedThinkingGroup(item)) return item.groupKey;
  return item.id;
}

function formatTokens(n: number | undefined): string {
  if (n == null) return "-";
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`;
  if (n >= 1_000) return `${(n / 1_000).toFixed(1)}K`;
  return String(n);
}

function getArtifactStatus(artifact: Artifact): string | null {
  if (!artifact.content || typeof artifact.content !== "object" || Array.isArray(artifact.content)) {
    return null;
  }
  const status = (artifact.content as Record<string, unknown>).status;
  return typeof status === "string" ? status : null;
}

function collectToolExecutionStats(artifacts: Artifact[]): ToolExecutionStats {
  const stats: ToolExecutionStats = {
    total: 0,
    completed: 0,
    failed: 0,
    running: 0,
    pending: 0,
  };

  for (const artifact of artifacts) {
    if (artifact.artifact_type !== "tool_execution") continue;
    stats.total += 1;
    const status = getArtifactStatus(artifact);
    if (status === "completed") stats.completed += 1;
    else if (status === "failed" || status === "rejected" || status === "canceled") stats.failed += 1;
    else if (status === "in_progress") stats.running += 1;
    else stats.pending += 1;
  }

  return stats;
}

function buildUsageSummary(usage: TokenUsageInfo | null): string {
  if (!usage) return "上下文用量: -";
  const { totalTokens, maxTokens, inputTokens, outputTokens } = usage;
  if (totalTokens == null && inputTokens == null && outputTokens == null) return "上下文用量: -";

  const ratio =
    totalTokens != null && maxTokens != null && maxTokens > 0
      ? ` (${Math.min(Math.round((totalTokens / maxTokens) * 100), 100)}%)`
      : "";
  const io =
    inputTokens != null || outputTokens != null
      ? ` · ↑${formatTokens(inputTokens)} ↓${formatTokens(outputTokens)}`
      : "";
  const total =
    totalTokens != null && maxTokens != null
      ? ` ${formatTokens(totalTokens)}/${formatTokens(maxTokens)}`
      : "";
  return `上下文用量:${total}${ratio}${io}`;
}

export function TaskAgentSessionPanel({ task, onTaskUpdated }: TaskAgentSessionPanelProps) {
  const navigate = useNavigate();
  const startTaskExecution = useStoryStore((state) => state.startTaskExecution);
  const continueTaskExecution = useStoryStore((state) => state.continueTaskExecution);
  const cancelTaskExecution = useStoryStore((state) => state.cancelTaskExecution);
  const refreshTask = useStoryStore((state) => state.refreshTask);
  const storeError = useStoryStore((state) => state.error);

  const [prompt, setPrompt] = useState("");
  const [submitError, setSubmitError] = useState<string | null>(null);
  const [isSubmitting, setIsSubmitting] = useState(false);

  const hasSession = Boolean(task.session_id);
  const sessionId = task.session_id ?? null;
  const executorSessionId = task.executor_session_id ?? null;
  const streamSessionId = sessionId ?? "__placeholder__";
  const executionLocked = task.status === "running";

  const {
    displayItems,
    rawEntries,
    isConnected,
    isLoading,
    isReceiving,
    error: streamError,
    reconnect,
    streamingEntryId,
    tokenUsage,
  } = useAcpSession({ sessionId: streamSessionId, enabled: hasSession });

  const listRef = useRef<HTMLDivElement>(null);
  const shouldAutoScrollRef = useRef(true);
  const taskSnapshotRef = useRef({ status: task.status, updated_at: task.updated_at });
  const onTaskUpdatedRef = useRef(onTaskUpdated);

  useEffect(() => {
    taskSnapshotRef.current = { status: task.status, updated_at: task.updated_at };
  }, [task.status, task.updated_at]);

  useEffect(() => {
    onTaskUpdatedRef.current = onTaskUpdated;
  }, [onTaskUpdated]);

  useEffect(() => {
    if (!listRef.current || !shouldAutoScrollRef.current) return;
    listRef.current.scrollTop = listRef.current.scrollHeight;
  }, [displayItems.length]);

  useEffect(() => {
    if (!hasSession || !executionLocked) return;

    const timer = window.setInterval(() => {
      void (async () => {
        const latest = await refreshTask(task.id);
        if (!latest) return;
        if (latest.status !== task.status || latest.updated_at !== task.updated_at) {
          onTaskUpdatedRef.current(latest);
        }
      })();
    }, 2000);

    return () => window.clearInterval(timer);
  }, [executionLocked, hasSession, refreshTask, task.id, task.status, task.updated_at]);

  useEffect(() => {
    if (!hasSession) return;
    let cancelled = false;
    void (async () => {
      const latest = await refreshTask(task.id);
      if (cancelled || !latest) return;
      if (
        latest.status !== taskSnapshotRef.current.status ||
        latest.updated_at !== taskSnapshotRef.current.updated_at
      ) {
        onTaskUpdatedRef.current(latest);
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [hasSession, refreshTask, task.id]);

  useEffect(() => {
    if (!hasSession || rawEntries.length === 0) return;

    const last = rawEntries[rawEntries.length - 1];
    if (!last || last.update.sessionUpdate !== "session_info_update") return;

    const meta = extractAgentDashMetaFromUpdate(last.update);
    const eventType = meta?.event?.type;
    if (eventType !== "turn_completed" && eventType !== "turn_failed") return;

    void (async () => {
      const latest = await refreshTask(task.id);
      if (latest) onTaskUpdatedRef.current(latest);
    })();
  }, [hasSession, rawEntries, refreshTask, task.id]);

  useEffect(() => {
    setSubmitError(null);
    setPrompt("");
  }, [task.id]);

  const handleScroll = useCallback(() => {
    const element = listRef.current;
    if (!element) return;
    shouldAutoScrollRef.current = element.scrollHeight - element.scrollTop - element.clientHeight < 48;
  }, []);

  const toolStats = useMemo(() => collectToolExecutionStats(task.artifacts), [task.artifacts]);
  const usageSummary = useMemo(() => buildUsageSummary(tokenUsage), [tokenUsage]);

  const connectionLabel = !hasSession
    ? "未创建"
    : isConnected
      ? "已连接"
      : isLoading
        ? "连接中"
        : "连接中断";
  const connectionDotClassName = !hasSession
    ? "bg-muted-foreground"
    : isConnected
      ? "bg-emerald-500"
      : isLoading
        ? "animate-pulse bg-amber-400"
        : "bg-destructive";

  const displayError = submitError ?? streamError?.message ?? null;
  const navigateToSessionPage = useCallback(
    (targetSessionId: string) => {
      const state: SessionNavigationState = {
        task_context: {
          task_id: task.id,
          agent_binding: task.agent_binding,
        },
        return_to: {
          story_id: task.story_id,
          task_id: task.id,
        },
      };
      navigate(`/session/${targetSessionId}`, { state });
    },
    [navigate, task.agent_binding, task.id, task.story_id],
  );

  const handleExecute = useCallback(async () => {
    if (isSubmitting) return;
    if (executionLocked) {
      setSubmitError("任务仍在执行中，请先等待完成或点击“取消执行”。");
      return;
    }
    setSubmitError(null);

    const trimmedPrompt = prompt.trim();
    setIsSubmitting(true);
    try {
      const updated = hasSession
        ? await continueTaskExecution(task.id, trimmedPrompt ? { additional_prompt: trimmedPrompt } : undefined)
        : await startTaskExecution(task.id, trimmedPrompt ? { override_prompt: trimmedPrompt } : undefined);
      if (!updated) {
        setSubmitError(storeError ?? (hasSession ? "继续执行失败，请稍后重试" : "启动执行失败，请稍后重试"));
        return;
      }
      onTaskUpdated(updated);
      setPrompt("");
    } catch (error) {
      setSubmitError(error instanceof Error ? error.message : "执行失败，请重试");
    } finally {
      setIsSubmitting(false);
    }
  }, [
    continueTaskExecution,
    executionLocked,
    hasSession,
    isSubmitting,
    onTaskUpdated,
    prompt,
    startTaskExecution,
    storeError,
    task.id,
  ]);

  const handleCancel = useCallback(async () => {
    if (!hasSession || isSubmitting || !executionLocked) return;
    setSubmitError(null);

    setIsSubmitting(true);
    try {
      const updated = await cancelTaskExecution(task.id);
      if (updated) {
        onTaskUpdated(updated);
      } else {
        setSubmitError("取消执行失败，请稍后重试");
      }
    } catch (error) {
      setSubmitError(error instanceof Error ? error.message : "取消执行失败，请稍后重试");
    } finally {
      setIsSubmitting(false);
    }
  }, [cancelTaskExecution, executionLocked, hasSession, isSubmitting, onTaskUpdated, task.id]);

  const handleRefresh = useCallback(async () => {
    setSubmitError(null);
    const latest = await refreshTask(task.id);
    if (!latest) {
      setSubmitError(storeError ?? "刷新任务状态失败，请稍后重试");
      return;
    }
    onTaskUpdatedRef.current(latest);
  }, [refreshTask, storeError, task.id]);

  return (
    <div className="space-y-3">
      <div className="flex flex-wrap items-center gap-2">
        <TaskStatusBadge status={task.status} />
        <span className="inline-flex items-center gap-1 rounded-full border border-border bg-background px-2 py-0.5 text-xs text-muted-foreground">
          <span className={`h-1.5 w-1.5 rounded-full ${connectionDotClassName}`} />
          {connectionLabel}
        </span>
        {isReceiving && <span className="text-xs text-primary">实时输出中</span>}
        <div className="ml-auto flex flex-col items-end text-xs text-muted-foreground">
          <span>Session: {sessionId ? `${sessionId.slice(0, 12)}...` : "未绑定"}</span>
          <span>
            Executor: {executorSessionId ? `${executorSessionId.slice(0, 24)}...` : "未绑定"}
          </span>
        </div>
      </div>

      <div className="grid gap-2 text-xs text-muted-foreground sm:grid-cols-2 xl:grid-cols-4">
        <div className="rounded-md border border-border bg-background px-2.5 py-2">
          <p>工具进度</p>
          <p className="mt-0.5 text-sm font-medium text-foreground">
            {toolStats.completed}/{toolStats.total || 0}
          </p>
        </div>
        <div className="rounded-md border border-border bg-background px-2.5 py-2">
          <p>运行中</p>
          <p className="mt-0.5 text-sm font-medium text-foreground">{toolStats.running}</p>
        </div>
        <div className="rounded-md border border-border bg-background px-2.5 py-2">
          <p>失败</p>
          <p className="mt-0.5 text-sm font-medium text-destructive">{toolStats.failed}</p>
        </div>
        <div className="rounded-md border border-border bg-background px-2.5 py-2">
          <p>产物总数</p>
          <p className="mt-0.5 text-sm font-medium text-foreground">{task.artifacts.length}</p>
        </div>
      </div>

      <p className="text-xs text-muted-foreground">{usageSummary}</p>

      {displayError && (
        <div className="rounded-md border border-destructive/40 bg-destructive/10 px-3 py-2 text-xs text-destructive">
          {displayError}
        </div>
      )}

      <div
        ref={listRef}
        onScroll={handleScroll}
        className="max-h-[26rem] min-h-[16rem] overflow-y-auto rounded-md border border-border bg-background"
      >
        {hasSession && isLoading && displayItems.length === 0 ? (
          <div className="flex h-[16rem] items-center justify-center">
            <div className="text-center">
              <div className="mx-auto h-6 w-6 animate-spin rounded-full border-2 border-primary border-t-transparent" />
              <p className="mt-2 text-xs text-muted-foreground">正在连接会话流...</p>
            </div>
          </div>
        ) : hasSession && displayItems.length > 0 ? (
          <div className="space-y-2 px-3 py-3">
            {displayItems.map((item) => (
              <AcpSessionEntry key={getItemKey(item)} item={item} streamingEntryId={streamingEntryId} />
            ))}
          </div>
        ) : (
          <div className="flex h-[16rem] items-center justify-center px-6 text-center text-sm text-muted-foreground">
            {hasSession ? "会话已建立，等待新的执行输出" : "填写指令后启动执行，会在此实时展示 Agent 输出"}
          </div>
        )}
      </div>

      <div className="space-y-2">
        <textarea
          value={prompt}
          onChange={(event) => setPrompt(event.target.value)}
          rows={3}
          placeholder={hasSession ? "输入追加指令（可选）" : "输入启动指令（可选）"}
          onKeyDown={(event) => {
            if ((event.ctrlKey || event.metaKey) && event.key === "Enter") {
              event.preventDefault();
              void handleExecute();
            }
          }}
          className="w-full rounded-md border border-border bg-background px-3 py-2 text-sm outline-none ring-ring focus:ring-1"
        />
        <div className="flex flex-wrap items-center justify-between gap-2">
          <p className="text-xs text-muted-foreground">Ctrl+Enter 快速执行</p>
          <div className="flex flex-wrap items-center gap-2">
            {hasSession && !isConnected && (
              <button
                type="button"
                onClick={reconnect}
                className="rounded border border-border bg-background px-2.5 py-1.5 text-xs text-foreground hover:bg-muted"
              >
                重新连接
              </button>
            )}
            {hasSession && (
              <button
                type="button"
                disabled={isSubmitting}
                onClick={() => void handleRefresh()}
                className="rounded border border-border bg-background px-2.5 py-1.5 text-xs text-foreground hover:bg-muted disabled:opacity-50"
              >
                刷新状态
              </button>
            )}
            {hasSession && (
              <button
                type="button"
                onClick={() => navigateToSessionPage(sessionId!)}
                className="rounded border border-border bg-background px-2.5 py-1.5 text-xs text-foreground hover:bg-muted"
              >
                会话页
              </button>
            )}
            {hasSession && (
              <button
                type="button"
                disabled={isSubmitting || !executionLocked}
                onClick={() => void handleCancel()}
                className="rounded border border-border bg-background px-2.5 py-1.5 text-xs text-foreground hover:bg-muted disabled:opacity-50"
              >
                取消执行
              </button>
            )}
            <button
              type="button"
              disabled={isSubmitting}
              onClick={() => void handleExecute()}
              className="rounded bg-primary px-3 py-1.5 text-xs font-medium text-primary-foreground disabled:opacity-50"
            >
              {executionLocked ? "执行中..." : isSubmitting ? "提交中..." : hasSession ? "继续执行" : "启动执行"}
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}
