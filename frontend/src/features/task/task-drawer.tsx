import { useState } from "react";
import { useNavigate } from "react-router-dom";
import type { Artifact, Task, TaskStatus } from "../../types";
import { TaskStatusBadge } from "../../components/ui/status-badge";
import { useStoryStore } from "../../stores/storyStore";
import {
  DangerConfirmDialog,
  DetailMenu,
  DetailPanel,
  DetailSection,
} from "../../components/ui/detail-panel";

interface TaskDrawerProps {
  task: Task | null;
  onTaskUpdated: (task: Task) => void;
  onTaskDeleted: (taskId: string, storyId: string) => void;
  onClose: () => void;
}

function ArtifactBlock({ artifact }: { artifact: Artifact }) {
  return (
    <div className="rounded-md border border-border bg-card p-3">
      <div className="mb-2 flex items-center justify-between gap-2">
        <p className="text-xs font-medium text-muted-foreground">
          {artifact.artifact_type}
        </p>
        <span className="text-[11px] text-muted-foreground">
          {new Date(artifact.created_at).toLocaleString("zh-CN")}
        </span>
      </div>
      <pre className="overflow-auto whitespace-pre-wrap text-xs leading-relaxed text-foreground">
        {JSON.stringify(artifact.content, null, 2)}
      </pre>
    </div>
  );
}

export function TaskDrawer({
  task,
  onTaskUpdated,
  onTaskDeleted,
  onClose,
}: TaskDrawerProps) {
  const navigate = useNavigate();
  const {
    updateTask,
    startTaskExecution,
    continueTaskExecution,
    deleteTask,
    error,
  } = useStoryStore();
  const [editTitle, setEditTitle] = useState(task?.title ?? "");
  const [editDescription, setEditDescription] = useState(task?.description ?? "");
  const [editStatus, setEditStatus] = useState<TaskStatus>(task?.status ?? "pending");
  const [formMessage, setFormMessage] = useState<string | null>(null);
  const [isDeleteConfirmOpen, setIsDeleteConfirmOpen] = useState(false);
  const [deleteConfirmValue, setDeleteConfirmValue] = useState("");
  const [isExecuting, setIsExecuting] = useState(false);

  if (!task) return null;

  const agentLabel = task.agent_binding?.agent_type ?? "未指定 Agent";

  const handleSaveTask = async () => {
    const trimmedTitle = editTitle.trim();
    if (!trimmedTitle) {
      setFormMessage("Task 标题不能为空");
      return;
    }

    const updated = await updateTask(task.id, {
      title: trimmedTitle,
      description: editDescription,
      status: editStatus,
    });
    if (!updated) return;

    setFormMessage(null);
    onTaskUpdated(updated);
  };

  const handleDeleteTask = async () => {
    if (deleteConfirmValue.trim() !== task.title) {
      setFormMessage("请输入完整 Task 标题后再删除");
      return;
    }
    await deleteTask(task.id, task.story_id);
    setIsDeleteConfirmOpen(false);
    onTaskDeleted(task.id, task.story_id);
  };

  const handleStartExecution = async () => {
    setIsExecuting(true);
    try {
      const updated = await startTaskExecution(task.id);
      if (!updated) return;
      setFormMessage(null);
      onTaskUpdated(updated);
      if (updated.session_id) {
        navigate(`/session/${updated.session_id}`);
      }
    } finally {
      setIsExecuting(false);
    }
  };

  const handleContinueExecution = async () => {
    setIsExecuting(true);
    try {
      const updated = await continueTaskExecution(task.id);
      if (!updated) return;
      setFormMessage(null);
      onTaskUpdated(updated);
      if (updated.session_id) {
        navigate(`/session/${updated.session_id}`);
      }
    } finally {
      setIsExecuting(false);
    }
  };

  return (
    <>
      <DetailPanel
        open={Boolean(task)}
        title={task.title}
        subtitle={`ID: ${task.id}`}
        onClose={onClose}
        widthClassName="max-w-[52rem]"
        overlayClassName="z-30"
        panelClassName="z-40"
        headerExtra={
          <div className="flex items-center gap-2">
            <TaskStatusBadge status={task.status} />
            <span className="text-xs text-muted-foreground">{agentLabel}</span>
            <DetailMenu
              items={[
                {
                  key: "delete",
                  label: "删除 Task",
                  danger: true,
                  onSelect: () => setIsDeleteConfirmOpen(true),
                },
              ]}
            />
          </div>
        }
      >
        <div className="space-y-4 p-5">
          <DetailSection title="任务详情">
            <input
              value={editTitle}
              onChange={(event) => setEditTitle(event.target.value)}
              placeholder="Task 标题"
              className="w-full rounded border border-border bg-background px-2 py-1.5 text-sm outline-none ring-ring focus:ring-1"
            />
            <textarea
              value={editDescription}
              onChange={(event) => setEditDescription(event.target.value)}
              rows={3}
              placeholder="Task 描述"
              className="w-full rounded border border-border bg-background px-2 py-1.5 text-sm outline-none ring-ring focus:ring-1"
            />
            <select
              value={editStatus}
              onChange={(event) => setEditStatus(event.target.value as TaskStatus)}
              className="w-full rounded border border-border bg-background px-2 py-1.5 text-sm outline-none ring-ring focus:ring-1"
            >
              <option value="pending">待执行</option>
              <option value="assigned">已分配</option>
              <option value="running">执行中</option>
              <option value="awaiting_verification">待验收</option>
              <option value="completed">已完成</option>
              <option value="failed">失败</option>
            </select>

            <div className="grid grid-cols-2 gap-3">
              <div className="rounded-md border border-border bg-background p-3">
                <p className="text-xs text-muted-foreground">工作空间 ID</p>
                <p className="mt-1 truncate text-sm font-mono text-foreground">
                  {task.workspace_id ?? "未绑定"}
                </p>
              </div>
              <div className="rounded-md border border-border bg-background p-3">
                <p className="text-xs text-muted-foreground">Agent 预设</p>
                <p className="mt-1 text-sm text-foreground">
                  {task.agent_binding?.preset_name ?? "无"}
                </p>
              </div>
            </div>
            <div className="flex justify-end">
              <button
                type="button"
                onClick={() => void handleSaveTask()}
                className="rounded border border-border bg-secondary px-3 py-1.5 text-sm text-foreground hover:bg-secondary/70"
              >
                保存 Task
              </button>
            </div>
          </DetailSection>

          <DetailSection title="执行操作">
            <div className="flex flex-wrap gap-2">
              {!task.session_id ? (
                <button
                  type="button"
                  disabled={isExecuting || task.status === "running"}
                  onClick={() => void handleStartExecution()}
                  className="rounded bg-primary px-3 py-1.5 text-sm text-primary-foreground disabled:opacity-50"
                >
                  {isExecuting ? "启动中..." : "启动执行"}
                </button>
              ) : (
                <button
                  type="button"
                  disabled={isExecuting || task.status === "running"}
                  onClick={() => void handleContinueExecution()}
                  className="rounded bg-primary px-3 py-1.5 text-sm text-primary-foreground disabled:opacity-50"
                >
                  {isExecuting ? "提交中..." : "继续执行"}
                </button>
              )}
              {task.session_id && (
                <button
                  type="button"
                  onClick={() => navigate(`/session/${task.session_id}`)}
                  className="rounded border border-border bg-background px-3 py-1.5 text-sm text-foreground hover:bg-muted"
                >
                  打开会话
                </button>
              )}
            </div>
            <p className="text-xs text-muted-foreground">
              Session: {task.session_id ?? "未绑定"}
            </p>
          </DetailSection>

          <DetailSection title="执行产物">
            {task.artifacts.length === 0 ? (
              <p className="rounded-md border border-dashed border-border px-3 py-6 text-center text-sm text-muted-foreground">
                暂无执行产物
              </p>
            ) : (
              <div className="space-y-2">
                {task.artifacts.map((artifact) => (
                  <ArtifactBlock key={artifact.id} artifact={artifact} />
                ))}
              </div>
            )}
          </DetailSection>

          {(formMessage || error) && (
            <p className="text-xs text-destructive">{formMessage || error}</p>
          )}
        </div>
      </DetailPanel>

      <DangerConfirmDialog
        open={isDeleteConfirmOpen}
        title="删除 Task"
        description="删除后不可恢复，请确认。"
        expectedValue={task.title}
        inputValue={deleteConfirmValue}
        onInputValueChange={setDeleteConfirmValue}
        confirmLabel="确认删除"
        onClose={() => {
          setIsDeleteConfirmOpen(false);
          setDeleteConfirmValue("");
        }}
        onConfirm={() => void handleDeleteTask()}
      />
    </>
  );
}
