/**
 * Task SubjectExecution 面板。
 *
 * Task 本身只作为 SubjectRef，运行状态由 lifecycle target view 投影。
 */

import { useCallback, useEffect, useMemo, useRef } from "react";
import { useNavigate } from "react-router-dom";
import type { SubjectExecutionView, Task } from "../../types";
import { subjectExecutionKey } from "../../types";
import { useLifecycleStore } from "../../stores/lifecycleStore";
import { useStoryStore } from "../../stores/storyStore";
import { agentRunWorkspacePath } from "../agent/agent-run-paths";

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
  const latestRuntimeNode = view.latest_runtime_node;

  return (
    <div className="space-y-3">
      <div className="grid gap-2 md:grid-cols-2">
        <div className="rounded-[8px] border border-border bg-background p-3">
          <p className="text-[10px] font-semibold uppercase tracking-wide text-muted-foreground">Current Agent</p>
          {currentAgent ? (
            <button
              type="button"
              onClick={() => navigate(agentRunWorkspacePath(
                currentAgent.agent_ref.run_id,
                currentAgent.agent_ref.agent_id,
              ))}
              className="mt-2 block w-full truncate text-left font-mono text-xs text-primary hover:underline"
            >
              {currentAgent.agent_ref.agent_id}
            </button>
          ) : (
            <p className="mt-2 text-xs text-muted-foreground">未分配 Agent</p>
          )}
        </div>
        <div className="rounded-[8px] border border-border bg-background p-3">
          <p className="text-[10px] font-semibold uppercase tracking-wide text-muted-foreground">Latest Runtime Node</p>
          {latestRuntimeNode ? (
            <p className="mt-2 text-xs text-foreground">
              {latestRuntimeNode.node_path} #{latestRuntimeNode.attempt} · {latestRuntimeNode.status}
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
                    <span
                      key={ref.runtime_session_id}
                      className="rounded-[6px] border border-border bg-secondary/40 px-1.5 py-0.5 font-mono text-[10px] text-muted-foreground"
                    >
                      RuntimeSession trace {ref.runtime_session_id.slice(0, 8)}
                    </span>
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
  const refreshTask = useStoryStore((s) => s.refreshTask);
  const fetchSubjectExecution = useLifecycleStore((s) => s.fetchSubjectExecution);
  const view = useLifecycleStore((s) => s.subjectExecutions.get(subjectExecutionKey("task", task.id)) ?? null);

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
          <button
            type="button"
            onClick={() => {
              void syncTask();
              void reloadExecution();
            }}
            className="rounded-[8px] border border-border bg-background px-2 py-1 text-xs text-muted-foreground transition-colors hover:bg-secondary hover:text-foreground"
          >
            刷新
          </button>
        </div>
      </div>

      <div className="min-h-0 flex-1 overflow-y-auto p-3">
        <SubjectExecutionSummary view={view} />
      </div>
    </div>
  );
}
