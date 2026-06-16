import { useCallback, useEffect, useMemo, useState } from "react";
import { Button } from "@agentdash/ui";

import type { Task, TaskPlanStatus, TaskPriority } from "../../types";
import { TaskStatusBadge } from "../../components/ui/status-badge";
import { useTaskPlanStore } from "../../stores/taskPlanStore";
import { TaskDrawer } from "./task-drawer";

interface TaskPlanPanelProps {
  runId: string | null;
  agentId: string | null;
}

const PRIORITY_OPTIONS: TaskPriority[] = ["p0", "p1", "p2", "p3"];

function nextStatus(status: TaskPlanStatus): TaskPlanStatus | null {
  switch (status) {
    case "open":
      return "active";
    case "active":
      return "review";
    case "review":
      return "done";
    case "blocked":
      return "active";
    case "done":
    case "dropped":
      return null;
  }
}

function statusLabel(status: TaskPlanStatus): string {
  switch (status) {
    case "open":
      return "开始";
    case "active":
      return "送审";
    case "review":
      return "完成";
    case "blocked":
      return "解除阻塞";
    case "done":
    case "dropped":
      return "";
  }
}

function isTaskPriority(value: string): value is TaskPriority {
  return value === "p0" || value === "p1" || value === "p2" || value === "p3";
}

export function TaskPlanPanel({ runId, agentId }: TaskPlanPanelProps) {
  const {
    taskPlansByRunId,
    isLoading,
    error,
    fetchAgentRunTasks,
    createAgentRunTask,
    updateTaskStatus,
    archiveTask,
  } = useTaskPlanStore();
  const [isExpanded, setIsExpanded] = useState(true);
  const [isCreating, setIsCreating] = useState(false);
  const [title, setTitle] = useState("");
  const [body, setBody] = useState("");
  const [priority, setPriority] = useState<TaskPriority | "">("");
  const [assignedAgentId, setAssignedAgentId] = useState("");
  const [selectedTaskId, setSelectedTaskId] = useState<string | null>(null);
  const [message, setMessage] = useState<string | null>(null);

  useEffect(() => {
    if (!runId || !agentId) return;
    void fetchAgentRunTasks(runId, agentId);
  }, [agentId, fetchAgentRunTasks, runId]);

  const tasks = useMemo(() => {
    if (!runId) return [];
    const plan = taskPlansByRunId[runId];
    return [...(plan?.tasks ?? [])]
      .filter((task) => !task.archived_at)
      .sort((a, b) => new Date(b.updated_at).getTime() - new Date(a.updated_at).getTime());
  }, [runId, taskPlansByRunId]);

  const selectedTask = useMemo(
    () => tasks.find((task) => task.id === selectedTaskId) ?? null,
    [selectedTaskId, tasks],
  );

  const handleCreate = useCallback(async () => {
    if (!runId || !agentId) return;
    const trimmedTitle = title.trim();
    if (!trimmedTitle) {
      setMessage("Task 标题不能为空");
      return;
    }

    const created = await createAgentRunTask(runId, agentId, {
      title: trimmedTitle,
      body: body.trim() || undefined,
      priority: priority || undefined,
      assigned_agent_id: assignedAgentId.trim() || undefined,
      context_refs: [],
    });
    if (!created) return;

    setTitle("");
    setBody("");
    setPriority("");
    setAssignedAgentId("");
    setIsCreating(false);
    setMessage(null);
  }, [agentId, assignedAgentId, body, createAgentRunTask, priority, runId, title]);

  const handleAdvance = useCallback(async (task: Task) => {
    const status = nextStatus(task.status);
    if (!status) return;
    await updateTaskStatus(task.owning_run_id, task.id, status);
  }, [updateTaskStatus]);

  const handleDrop = useCallback(async (task: Task) => {
    await archiveTask(task.owning_run_id, task.id);
  }, [archiveTask]);

  if (!runId || !agentId) return null;

  return (
    <section className="shrink-0 border-b border-border bg-background">
      <div className="flex items-center justify-between gap-3 px-4 py-2.5">
        <div className="min-w-0">
          <div className="flex items-center gap-2">
            <span className="agentdash-panel-header-tag">Task Plan</span>
            <span className="rounded-[6px] border border-border bg-secondary px-1.5 py-0.5 text-[10px] text-muted-foreground">
              {tasks.length}
            </span>
          </div>
          <p className="mt-1 truncate text-xs text-muted-foreground">
            Run-scoped plan items
          </p>
        </div>
        <div className="flex shrink-0 items-center gap-2">
          <Button type="button" variant="secondary" size="sm" onClick={() => setIsCreating((value) => !value)}>
            {isCreating ? "取消" : "新建"}
          </Button>
          <Button type="button" variant="ghost" size="sm" onClick={() => setIsExpanded((value) => !value)}>
            {isExpanded ? "收起" : "展开"}
          </Button>
        </div>
      </div>

      {isExpanded && (
        <div className="space-y-2 border-t border-border px-4 py-3">
          {isCreating && (
            <div className="grid gap-2 rounded-[8px] border border-border bg-secondary/20 p-3 md:grid-cols-[minmax(0,1fr)_9rem_13rem_auto]">
              <div className="space-y-2">
                <input
                  value={title}
                  onChange={(event) => setTitle(event.target.value)}
                  placeholder="Task 标题"
                  className="agentdash-form-input"
                />
                <textarea
                  value={body}
                  onChange={(event) => setBody(event.target.value)}
                  rows={2}
                  placeholder="计划说明"
                  className="agentdash-form-textarea"
                />
              </div>
              <select
                value={priority}
                onChange={(event) => {
                  const value = event.target.value;
                  setPriority(isTaskPriority(value) ? value : "");
                }}
                className="agentdash-form-select"
              >
                <option value="">priority</option>
                {PRIORITY_OPTIONS.map((item) => (
                  <option key={item} value={item}>{item}</option>
                ))}
              </select>
              <input
                value={assignedAgentId}
                onChange={(event) => setAssignedAgentId(event.target.value)}
                placeholder="assigned_agent_id"
                className="agentdash-form-input font-mono"
              />
              <Button type="button" variant="secondary" size="sm" onClick={() => void handleCreate()}>
                创建
              </Button>
            </div>
          )}

          {tasks.length === 0 ? (
            <div className="rounded-[8px] border border-dashed border-border bg-secondary/20 px-3 py-4 text-center text-sm text-muted-foreground">
              当前 AgentRun 暂无计划项
            </div>
          ) : (
            <div className="max-h-56 space-y-2 overflow-y-auto pr-1">
              {tasks.map((task) => {
                const advanceStatus = nextStatus(task.status);
                return (
                  <div
                    key={task.id}
                    className="grid gap-2 rounded-[8px] border border-border bg-background p-3 md:grid-cols-[minmax(0,1fr)_auto_auto]"
                  >
                    <button
                      type="button"
                      onClick={() => setSelectedTaskId(task.id)}
                      className="min-w-0 text-left"
                    >
                      <div className="flex min-w-0 items-center gap-2">
                        <TaskStatusBadge status={task.status} className="shrink-0 px-2 py-0.5 text-[11px]" />
                        <span className="truncate text-sm font-medium text-foreground">{task.title}</span>
                        {task.priority && (
                          <span className="rounded-[6px] border border-border bg-secondary px-1.5 py-0.5 text-[10px] text-muted-foreground">
                            {task.priority}
                          </span>
                        )}
                      </div>
                      <p className="mt-1 truncate text-xs text-muted-foreground">
                        {task.body || task.assigned_agent_id || task.owner_agent_id || task.id}
                      </p>
                    </button>
                    {advanceStatus && (
                      <Button type="button" variant="secondary" size="sm" onClick={() => void handleAdvance(task)}>
                        {statusLabel(task.status)}
                      </Button>
                    )}
                    <Button type="button" variant="ghost" size="sm" onClick={() => void handleDrop(task)}>
                      归档
                    </Button>
                  </div>
                );
              })}
            </div>
          )}

          {(message || error || isLoading) && (
            <p className={`text-xs ${message || error ? "text-destructive" : "text-muted-foreground"}`}>
              {message || error || "正在刷新 Task plan..."}
            </p>
          )}
        </div>
      )}

      <TaskDrawer
        task={selectedTask}
        onTaskUpdated={(task) => {
          setSelectedTaskId(task.id);
        }}
        onTaskDeleted={(taskId) => {
          if (selectedTaskId === taskId) setSelectedTaskId(null);
        }}
        onClose={() => setSelectedTaskId(null)}
      />
    </section>
  );
}
