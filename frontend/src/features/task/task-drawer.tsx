import { useState } from "react";
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
  if (artifact.type === "text") {
    return (
      <div className="rounded-md border border-border bg-card p-3">
        <p className="mb-2 text-xs font-medium text-muted-foreground">
          {artifact.title ?? "文本产物"}
        </p>
        <pre className="whitespace-pre-wrap text-xs leading-relaxed text-foreground">
          {artifact.content}
        </pre>
      </div>
    );
  }

  if (artifact.type === "content_block") {
    return (
      <div className="rounded-md border border-border bg-card p-3">
        <p className="mb-2 text-xs font-medium text-muted-foreground">
          {artifact.title ?? "内容块产物"}
        </p>
        <pre className="whitespace-pre-wrap text-xs text-foreground">
          {artifact.blocks
            .map((block) => ("text" in block ? block.text : JSON.stringify(block)))
            .join("\n")}
        </pre>
      </div>
    );
  }

  return (
    <div className="rounded-md border border-border bg-card p-3">
      <p className="mb-2 text-xs font-medium text-muted-foreground">
        {artifact.title ?? "JSON 产物"}
      </p>
      <pre className="overflow-auto text-xs leading-relaxed text-foreground">
        {JSON.stringify(artifact.data, null, 2)}
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
  const { updateTask, deleteTask, error } = useStoryStore();
  const [editTitle, setEditTitle] = useState(task?.title ?? "");
  const [editDescription, setEditDescription] = useState(task?.description ?? "");
  const [editStatus, setEditStatus] = useState<TaskStatus>(task?.status ?? "pending");
  const [formMessage, setFormMessage] = useState<string | null>(null);
  const [isDeleteConfirmOpen, setIsDeleteConfirmOpen] = useState(false);
  const [deleteConfirmValue, setDeleteConfirmValue] = useState("");

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
              <option value="queued">排队中</option>
              <option value="running">执行中</option>
              <option value="succeeded">成功</option>
              <option value="failed">失败</option>
              <option value="skipped">已跳过</option>
              <option value="cancelled">已取消</option>
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

          <DetailSection title="执行产物">
            {task.artifacts.length === 0 ? (
              <p className="rounded-md border border-dashed border-border px-3 py-6 text-center text-sm text-muted-foreground">
                暂无执行产物
              </p>
            ) : (
              <div className="space-y-2">
                {task.artifacts.map((artifact, index) => (
                  <ArtifactBlock key={index} artifact={artifact} />
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
