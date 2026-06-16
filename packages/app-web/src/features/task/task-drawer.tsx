import { useEffect, useState } from "react";
import type { Task, TaskPlanStatus, TaskPriority } from "../../types";
import { TaskStatusBadge } from "../../components/ui/status-badge";
import { useTaskPlanStore } from "../../stores/taskPlanStore";
import {
  DangerConfirmDialog,
  DetailMenu,
  DetailPanel,
  DetailSection,
} from "@agentdash/ui";
import { TaskSubjectExecutionPanel } from "./task-subject-execution-panel";

interface TaskDrawerProps {
  task: Task | null;
  onTaskUpdated: (task: Task) => void;
  onTaskDeleted: (taskId: string) => void;
  onClose: () => void;
}

const TASK_STATUS_OPTIONS: TaskPlanStatus[] = [
  "open",
  "active",
  "review",
  "blocked",
  "done",
  "dropped",
];

const TASK_PRIORITY_OPTIONS: TaskPriority[] = ["p0", "p1", "p2", "p3"];

function isTaskPlanStatus(value: string): value is TaskPlanStatus {
  return value === "open"
    || value === "active"
    || value === "review"
    || value === "blocked"
    || value === "done"
    || value === "dropped";
}

function isTaskPriority(value: string): value is TaskPriority {
  return value === "p0" || value === "p1" || value === "p2" || value === "p3";
}

function OptionalMetaRow({ label, value }: { label: string; value?: string | null }) {
  return (
    <div className="rounded-[8px] border border-border bg-background px-3 py-2">
      <p className="text-[10px] font-medium text-muted-foreground">{label}</p>
      <p className="mt-1 truncate font-mono text-xs text-foreground">{value?.trim() || "未设置"}</p>
    </div>
  );
}

export function TaskDrawer({
  task,
  onTaskUpdated,
  onTaskDeleted,
  onClose,
}: TaskDrawerProps) {
  const { updateTask, updateTaskStatus, archiveTask, error } = useTaskPlanStore();
  const [editTitle, setEditTitle] = useState(task?.title ?? "");
  const [editBody, setEditBody] = useState(task?.body ?? "");
  const [editPriority, setEditPriority] = useState<TaskPriority | "">(task?.priority ?? "");
  const [editOwnerAgentId, setEditOwnerAgentId] = useState(task?.owner_agent_id ?? "");
  const [editAssignedAgentId, setEditAssignedAgentId] = useState(task?.assigned_agent_id ?? "");
  const [editSourceTaskId, setEditSourceTaskId] = useState(task?.source_task_id ?? "");
  const [editStatus, setEditStatus] = useState<TaskPlanStatus>(task?.status ?? "open");
  const [formMessage, setFormMessage] = useState<string | null>(null);
  const [isArchiveConfirmOpen, setIsArchiveConfirmOpen] = useState(false);
  const [archiveConfirmValue, setArchiveConfirmValue] = useState("");

  useEffect(() => {
    if (!task) return;
    setEditTitle(task.title);
    setEditBody(task.body ?? "");
    setEditPriority(task.priority ?? "");
    setEditOwnerAgentId(task.owner_agent_id ?? "");
    setEditAssignedAgentId(task.assigned_agent_id ?? "");
    setEditSourceTaskId(task.source_task_id ?? "");
    setEditStatus(task.status);
    setFormMessage(null);
    setIsArchiveConfirmOpen(false);
    setArchiveConfirmValue("");
  }, [task?.id]);

  if (!task) return null;

  const applyTaskSnapshot = (nextTask: Task) => {
    setEditTitle(nextTask.title);
    setEditBody(nextTask.body ?? "");
    setEditPriority(nextTask.priority ?? "");
    setEditOwnerAgentId(nextTask.owner_agent_id ?? "");
    setEditAssignedAgentId(nextTask.assigned_agent_id ?? "");
    setEditSourceTaskId(nextTask.source_task_id ?? "");
    setEditStatus(nextTask.status);
    setFormMessage(null);
  };

  const handleSaveTask = async () => {
    const trimmedTitle = editTitle.trim();
    if (!trimmedTitle) {
      setFormMessage("Task 标题不能为空");
      return;
    }

    const updated = await updateTask(task.owning_run_id, task.id, {
      title: trimmedTitle,
      body: editBody.trim() || null,
      priority: editPriority || null,
      owner_agent_id: editOwnerAgentId.trim() || null,
      assigned_agent_id: editAssignedAgentId.trim() || null,
      source_task_id: editSourceTaskId.trim() || null,
    });
    if (!updated) return;

    const statusUpdated = editStatus !== updated.status
      ? await updateTaskStatus(task.owning_run_id, task.id, editStatus)
      : updated;
    if (!statusUpdated) return;

    applyTaskSnapshot(statusUpdated);
    onTaskUpdated(statusUpdated);
  };

  const handleArchiveTask = async () => {
    if (archiveConfirmValue.trim() !== task.title) {
      setFormMessage("请输入完整 Task 标题后再归档");
      return;
    }
    const archived = await archiveTask(task.owning_run_id, task.id);
    if (!archived) return;

    setIsArchiveConfirmOpen(false);
    onTaskUpdated(archived);
    onTaskDeleted(task.id);
  };

  return (
    <>
      <DetailPanel
        open={Boolean(task)}
        title={task.title}
        subtitle={task.id.slice(0, 8)}
        onClose={onClose}
        widthClassName="max-w-[78rem]"
        overlayClassName="z-30"
        panelClassName="z-40"
        headerExtra={
          <div className="flex items-center gap-2">
            <TaskStatusBadge status={task.status} />
            <DetailMenu
              items={[
                {
                  key: "archive",
                  label: "归档 Task",
                  danger: true,
                  onSelect: () => setIsArchiveConfirmOpen(true),
                },
              ]}
            />
          </div>
        }
      >
        <div className="grid gap-4 p-5 lg:grid-cols-[24rem_minmax(0,1fr)]">
          <div className="space-y-4">
            <DetailSection title="计划字段">
              <input
                value={editTitle}
                onChange={(event) => setEditTitle(event.target.value)}
                placeholder="Task 标题"
                className="agentdash-form-input"
              />
              <textarea
                value={editBody}
                onChange={(event) => setEditBody(event.target.value)}
                rows={4}
                placeholder="Task body / 验收口径 / 实现边界"
                className="agentdash-form-textarea"
              />

              <div className="grid gap-2 sm:grid-cols-2">
                <label className="space-y-1">
                  <span className="agentdash-form-label">计划状态</span>
                  <select
                    value={editStatus}
                    onChange={(event) => {
                      const value = event.target.value;
                      if (isTaskPlanStatus(value)) setEditStatus(value);
                    }}
                    className="agentdash-form-select"
                  >
                    {TASK_STATUS_OPTIONS.map((status) => (
                      <option key={status} value={status}>{status}</option>
                    ))}
                  </select>
                </label>

                <label className="space-y-1">
                  <span className="agentdash-form-label">优先级</span>
                  <select
                    value={editPriority}
                    onChange={(event) => {
                      const value = event.target.value;
                      setEditPriority(isTaskPriority(value) ? value : "");
                    }}
                    className="agentdash-form-select"
                  >
                    <option value="">未设置</option>
                    {TASK_PRIORITY_OPTIONS.map((priority) => (
                      <option key={priority} value={priority}>{priority}</option>
                    ))}
                  </select>
                </label>
              </div>

              <div className="grid gap-2">
                <label className="space-y-1">
                  <span className="agentdash-form-label">Owner Agent ID</span>
                  <input
                    value={editOwnerAgentId}
                    onChange={(event) => setEditOwnerAgentId(event.target.value)}
                    placeholder="owner_agent_id"
                    className="agentdash-form-input font-mono"
                  />
                </label>
                <label className="space-y-1">
                  <span className="agentdash-form-label">Assigned Agent ID</span>
                  <input
                    value={editAssignedAgentId}
                    onChange={(event) => setEditAssignedAgentId(event.target.value)}
                    placeholder="assigned_agent_id"
                    className="agentdash-form-input font-mono"
                  />
                </label>
                <label className="space-y-1">
                  <span className="agentdash-form-label">Source Task ID</span>
                  <input
                    value={editSourceTaskId}
                    onChange={(event) => setEditSourceTaskId(event.target.value)}
                    placeholder="source_task_id"
                    className="agentdash-form-input font-mono"
                  />
                </label>
              </div>

              <div className="flex justify-end">
                <button
                  type="button"
                  onClick={() => void handleSaveTask()}
                  className="agentdash-button-secondary"
                >
                  保存计划项
                </button>
              </div>
            </DetailSection>

            <DetailSection title="关联">
              <div className="grid gap-2">
                <OptionalMetaRow label="Owning Run" value={task.owning_run_id} />
                <OptionalMetaRow label="Created By Agent" value={task.created_by_agent_id} />
                <OptionalMetaRow label="Owner Agent" value={task.owner_agent_id} />
                <OptionalMetaRow label="Assigned Agent" value={task.assigned_agent_id} />
                <OptionalMetaRow label="Story Ref" value={task.story_ref ? `${task.story_ref.kind}:${task.story_ref.id}` : null} />
              </div>
              {task.context_refs.length > 0 && (
                <div className="space-y-2">
                  <p className="text-xs font-medium text-muted-foreground">Context Refs</p>
                  {task.context_refs.map((context, index) => (
                    <div
                      key={`${context.kind}:${context.locator}:${index}`}
                      className="rounded-[8px] border border-border bg-background px-3 py-2"
                    >
                      <p className="text-xs font-medium text-foreground">
                        {context.label?.trim() || context.kind}
                      </p>
                      <p className="mt-1 truncate font-mono text-[11px] text-muted-foreground">
                        {context.locator}
                      </p>
                    </div>
                  ))}
                </div>
              )}
            </DetailSection>

            {(formMessage || error) && (
              <p className="text-xs text-destructive">{formMessage || error}</p>
            )}
          </div>

          <div className="space-y-4">
            <DetailSection
              title="Linked Runs"
              description="执行事实来自 SubjectExecutionView。"
            >
              <div className="h-[38rem] overflow-hidden rounded-[8px] border border-border">
                <TaskSubjectExecutionPanel task={task} />
              </div>
            </DetailSection>
          </div>
        </div>
      </DetailPanel>

      <DangerConfirmDialog
        open={isArchiveConfirmOpen}
        title="归档 Task"
        description="Task 会从默认计划视图隐藏，关联运行记录仍保留在执行投影中。"
        expectedValue={task.title}
        inputValue={archiveConfirmValue}
        onInputValueChange={setArchiveConfirmValue}
        confirmLabel="确认归档"
        onClose={() => {
          setIsArchiveConfirmOpen(false);
          setArchiveConfirmValue("");
        }}
        onConfirm={() => void handleArchiveTask()}
      />
    </>
  );
}
