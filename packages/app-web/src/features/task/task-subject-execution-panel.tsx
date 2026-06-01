/**
 * Task SubjectExecution 面板。
 *
 * Task 本身只作为 SubjectRef，运行状态由 lifecycle target view 投影。
 */

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useNavigate } from "react-router-dom";
import type { SubjectExecutionView, Task } from "../../types";
import { subjectExecutionKey } from "../../types";
import { useLifecycleStore } from "../../stores/lifecycleStore";
import { useStoryStore } from "../../stores/storyStore";

interface TaskSubjectExecutionPanelProps {
  task: Task;
  onTaskUpdated: (task: Task) => void;
}

function JsonBlock({ value }: { value: unknown }) {
  const text = useMemo(() => JSON.stringify(value ?? null, null, 2), [value]);
  return (
    <pre className="max-h-48 overflow-auto rounded-[8px] border border-border bg-secondary/20 p-3 text-xs text-muted-foreground">
      {text}
    </pre>
  );
}

function SubjectExecutionSummary({ view }: { view: SubjectExecutionView | null }) {
  const navigate = useNavigate();

  if (!view) {
    return (
      <div className="rounded-[8px] border border-dashed border-border bg-secondary/20 px-3 py-8 text-center text-sm text-muted-foreground">
        暂无 SubjectExecution 投影
      </div>
    );
  }

  const currentAgent = view.current_agent;
  const latestAttempt = view.latest_attempt;

  return (
    <div className="space-y-3">
      <div className="grid gap-2 md:grid-cols-2">
        <div className="rounded-[8px] border border-border bg-background p-3">
          <p className="text-[10px] font-semibold uppercase tracking-wide text-muted-foreground">Current Agent</p>
          {currentAgent ? (
            <button
              type="button"
              onClick={() => navigate(`/agent/${currentAgent.agent_ref.agent_id}`, {
                state: { run_id: currentAgent.agent_ref.run_id },
              })}
              className="mt-2 block w-full truncate text-left font-mono text-xs text-primary hover:underline"
            >
              {currentAgent.agent_ref.agent_id}
            </button>
          ) : (
            <p className="mt-2 text-xs text-muted-foreground">未分配 Agent</p>
          )}
        </div>
        <div className="rounded-[8px] border border-border bg-background p-3">
          <p className="text-[10px] font-semibold uppercase tracking-wide text-muted-foreground">Latest Attempt</p>
          {latestAttempt ? (
            <p className="mt-2 text-xs text-foreground">
              {latestAttempt.activity_key} #{latestAttempt.attempt} · {latestAttempt.status}
            </p>
          ) : (
            <p className="mt-2 text-xs text-muted-foreground">暂无执行记录</p>
          )}
        </div>
      </div>

      {view.runs.length > 0 && (
        <div className="space-y-2">
          <p className="text-[10px] font-semibold uppercase tracking-wide text-muted-foreground">Lifecycle Runs</p>
          {view.runs.map((run) => (
            <div
              key={run.run_ref.run_id}
              className="rounded-[8px] border border-border bg-background p-3"
            >
              <div className="flex items-center gap-2">
                <button
                  type="button"
                  onClick={() => navigate(`/run/${run.run_ref.run_id}`)}
                  className="truncate font-mono text-xs text-primary hover:underline"
                >
                  {run.run_ref.run_id}
                </button>
                <span className="rounded-[6px] border border-border bg-secondary px-1.5 py-0.5 text-[10px] text-muted-foreground">
                  {run.status}
                </span>
              </div>
              {run.runtime_trace_refs.length > 0 && (
                <div className="mt-2 flex flex-wrap gap-1.5">
                  {run.runtime_trace_refs.map((ref) => (
                    <button
                      key={ref.runtime_session_id}
                      type="button"
                      onClick={() => navigate(`/session/${ref.runtime_session_id}`)}
                      className="rounded-[6px] border border-border bg-secondary/40 px-1.5 py-0.5 font-mono text-[10px] text-muted-foreground hover:text-foreground"
                    >
                      trace {ref.runtime_session_id.slice(0, 8)}
                    </button>
                  ))}
                </div>
              )}
            </div>
          ))}
        </div>
      )}

      <div className="space-y-2">
        <p className="text-[10px] font-semibold uppercase tracking-wide text-muted-foreground">Artifacts</p>
        <JsonBlock value={view.artifacts} />
      </div>
    </div>
  );
}

export function TaskSubjectExecutionPanel({ task, onTaskUpdated }: TaskSubjectExecutionPanelProps) {
  const startTaskExecution = useStoryStore((s) => s.startTaskExecution);
  const continueTaskExecution = useStoryStore((s) => s.continueTaskExecution);
  const cancelTaskExecution = useStoryStore((s) => s.cancelTaskExecution);
  const refreshTask = useStoryStore((s) => s.refreshTask);
  const fetchSubjectExecution = useLifecycleStore((s) => s.fetchSubjectExecution);
  const view = useLifecycleStore((s) => s.subjectExecutions.get(subjectExecutionKey("task", task.id)) ?? null);

  const [prompt, setPrompt] = useState("");
  const [isBusy, setIsBusy] = useState(false);
  const [message, setMessage] = useState<string | null>(null);

  const onTaskUpdatedRef = useRef(onTaskUpdated);
  useEffect(() => {
    onTaskUpdatedRef.current = onTaskUpdated;
  }, [onTaskUpdated]);

  const reloadExecution = useCallback(async () => {
    await fetchSubjectExecution("task", task.id);
  }, [fetchSubjectExecution, task.id]);

  useEffect(() => {
    void reloadExecution();
  }, [reloadExecution, task.status, task.updated_at]);

  const syncTask = useCallback(async () => {
    const latest = await refreshTask(task.id);
    if (latest) onTaskUpdatedRef.current(latest);
  }, [refreshTask, task.id]);

  const runAction = useCallback(
    async (kind: "start" | "continue" | "cancel") => {
      setIsBusy(true);
      setMessage(null);
      try {
        let updated: Task | null = null;
        if (kind === "start") {
          updated = await startTaskExecution(task.id, prompt.trim() ? { override_prompt: prompt.trim() } : undefined);
        } else if (kind === "continue") {
          updated = await continueTaskExecution(task.id, prompt.trim() ? { additional_prompt: prompt.trim() } : undefined);
        } else {
          updated = await cancelTaskExecution(task.id);
        }
        if (updated) onTaskUpdatedRef.current(updated);
        await reloadExecution();
        if (kind !== "cancel") setPrompt("");
      } finally {
        setIsBusy(false);
      }
    },
    [cancelTaskExecution, continueTaskExecution, prompt, reloadExecution, startTaskExecution, task.id],
  );

  const canStart = task.status === "pending" || task.status === "assigned" || !view?.current_agent;
  const canContinue =
    Boolean(view?.current_agent) && task.status !== "completed" && task.status !== "failed" && task.status !== "cancelled";
  const canCancel = task.status === "running";

  return (
    <div className="flex h-full flex-col overflow-hidden bg-background">
      <div className="shrink-0 border-b border-border p-3">
        <div className="flex items-center gap-2">
          <span className="rounded-[6px] border border-border bg-secondary px-1.5 py-0.5 text-[10px] font-semibold uppercase tracking-wide text-muted-foreground">
            SubjectExecution
          </span>
          <span className="min-w-0 flex-1 truncate text-sm font-medium text-foreground">{task.title}</span>
          <span className="rounded-[6px] border border-border bg-secondary/40 px-1.5 py-0.5 text-[10px] text-muted-foreground">
            {task.status}
          </span>
        </div>
        <textarea
          value={prompt}
          onChange={(event) => setPrompt(event.target.value)}
          rows={3}
          placeholder="可选执行指令"
          className="mt-3 w-full resize-none rounded-[8px] border border-border bg-background px-3 py-2 text-sm text-foreground outline-none placeholder:text-muted-foreground focus:border-primary"
        />
        <div className="mt-2 flex flex-wrap items-center gap-2">
          <button
            type="button"
            onClick={() => void runAction("start")}
            disabled={isBusy || !canStart}
            className="agentdash-button-primary disabled:opacity-50"
          >
            启动执行
          </button>
          <button
            type="button"
            onClick={() => void runAction("continue")}
            disabled={isBusy || !canContinue}
            className="agentdash-button-secondary disabled:opacity-50"
          >
            继续执行
          </button>
          {canCancel && (
            <button
              type="button"
              onClick={() => void runAction("cancel")}
              disabled={isBusy}
              className="rounded-[8px] border border-destructive/40 bg-destructive/8 px-3 py-2 text-sm text-destructive transition-colors hover:bg-destructive/15 disabled:opacity-50"
            >
              取消
            </button>
          )}
          <button
            type="button"
            onClick={() => {
              void syncTask();
              void reloadExecution();
            }}
            disabled={isBusy}
            className="rounded-[8px] border border-border bg-background px-3 py-2 text-sm text-muted-foreground transition-colors hover:bg-secondary hover:text-foreground disabled:opacity-50"
          >
            刷新
          </button>
          {message && <span className="text-xs text-destructive">{message}</span>}
        </div>
      </div>

      <div className="min-h-0 flex-1 overflow-y-auto p-3">
        <SubjectExecutionSummary view={view} />
      </div>
    </div>
  );
}
