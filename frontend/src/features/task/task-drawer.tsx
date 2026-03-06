import { useMemo, useState } from "react";
import type { AgentBinding, Artifact, ProjectConfig, Task, TaskStatus, Workspace } from "../../types";
import { TaskStatusBadge } from "../../components/ui/status-badge";
import { useStoryStore } from "../../stores/storyStore";
import {
  DangerConfirmDialog,
  DetailMenu,
  DetailPanel,
  DetailSection,
} from "../../components/ui/detail-panel";
import { AgentBindingFields } from "./agent-binding-fields";
import {
  hasAgentBindingSelection,
  normalizeAgentBinding,
} from "./agent-binding";
import { TaskAgentSessionPanel } from "./task-agent-session-panel";

interface TaskDrawerProps {
  task: Task | null;
  workspaces: Workspace[];
  projectConfig?: ProjectConfig;
  onTaskUpdated: (task: Task) => void;
  onTaskDeleted: (taskId: string) => void;
  onClose: () => void;
}

function ArtifactBlock({ artifact }: { artifact: Artifact }) {
  const [isCollapsed, setIsCollapsed] = useState(true);

  return (
    <div className="rounded-[12px] border border-border bg-background p-3">
      <div className="mb-2 flex items-center justify-between gap-2">
        <p className="text-xs font-medium text-muted-foreground">
          {artifact.artifact_type}
        </p>
        <div className="flex items-center gap-2">
          <span className="text-[11px] text-muted-foreground">
            {new Date(artifact.created_at).toLocaleString("zh-CN")}
          </span>
          <button
            type="button"
            onClick={() => setIsCollapsed((value) => !value)}
            aria-expanded={!isCollapsed}
            className="rounded-[8px] border border-border bg-background px-2 py-1 text-[11px] text-muted-foreground transition-colors hover:bg-secondary"
          >
            {isCollapsed ? "展开" : "收起"}
          </button>
        </div>
      </div>
      {isCollapsed ? (
        <p className="text-xs text-muted-foreground">内容已折叠，点击展开查看详情</p>
      ) : (
        <pre className="agentdash-chat-code-block whitespace-pre-wrap">
          {JSON.stringify(artifact.content, null, 2)}
        </pre>
      )}
    </div>
  );
}

export function TaskDrawer({
  task,
  workspaces,
  projectConfig,
  onTaskUpdated,
  onTaskDeleted,
  onClose,
}: TaskDrawerProps) {
  const { updateTask, deleteTask, error } = useStoryStore();
  const [editTitle, setEditTitle] = useState(task?.title ?? "");
  const [editDescription, setEditDescription] = useState(task?.description ?? "");
  const [editStatus, setEditStatus] = useState<TaskStatus>(task?.status ?? "pending");
  const [editWorkspaceId, setEditWorkspaceId] = useState(task?.workspace_id ?? "");
  const [editAgentBinding, setEditAgentBinding] = useState<AgentBinding>(task?.agent_binding ?? {});
  const [formMessage, setFormMessage] = useState<string | null>(null);
  const [isDeleteConfirmOpen, setIsDeleteConfirmOpen] = useState(false);
  const [deleteConfirmValue, setDeleteConfirmValue] = useState("");
  const [isArtifactsCollapsed, setIsArtifactsCollapsed] = useState(true);
  const sortedArtifacts = useMemo(
    () => {
      if (!task) return [];
      return [...task.artifacts].sort(
        (a, b) => new Date(b.created_at).getTime() - new Date(a.created_at).getTime(),
      );
    },
    [task],
  );

  if (!task) return null;

  const agentLabel = task.agent_binding?.agent_type ?? "未指定 Agent";
  const hasArtifacts = sortedArtifacts.length > 0;

  const applyTaskSnapshot = (nextTask: Task) => {
    setEditTitle(nextTask.title);
    setEditDescription(nextTask.description ?? "");
    setEditStatus(nextTask.status);
    setEditWorkspaceId(nextTask.workspace_id ?? "");
    setEditAgentBinding(nextTask.agent_binding ?? {});
    setFormMessage(null);
  };

  const handleSaveTask = async () => {
    const trimmedTitle = editTitle.trim();
    if (!trimmedTitle) {
      setFormMessage("Task 标题不能为空");
      return;
    }
    if (!hasAgentBindingSelection(editAgentBinding, projectConfig)) {
      setFormMessage("请指定 Agent 类型或预设，或先在 Project 配置中设置 default_agent_type");
      return;
    }

    const updated = await updateTask(task.id, {
      title: trimmedTitle,
      description: editDescription,
      status: editStatus,
      workspace_id: editWorkspaceId || null,
      agent_binding: normalizeAgentBinding(editAgentBinding),
    });
    if (!updated) return;

    applyTaskSnapshot(updated);
    onTaskUpdated(updated);
  };

  const handleDeleteTask = async () => {
    if (deleteConfirmValue.trim() !== task.title) {
      setFormMessage("请输入完整 Task 标题后再删除");
      return;
    }
    await deleteTask(task.id, task.story_id);
    setIsDeleteConfirmOpen(false);
    onTaskDeleted(task.id);
  };

  return (
    <>
      <DetailPanel
        open={Boolean(task)}
        title={task.title}
        subtitle={`ID: ${task.id}`}
        onClose={onClose}
        widthClassName="max-w-[78rem]"
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
        <div className="grid gap-4 p-5 lg:grid-cols-[22rem_minmax(0,1fr)]">
          <div className="space-y-4">
            <DetailSection title="任务详情">
              <input
                value={editTitle}
                onChange={(event) => setEditTitle(event.target.value)}
                placeholder="Task 标题"
                className="agentdash-form-input"
              />
              <textarea
                value={editDescription}
                onChange={(event) => setEditDescription(event.target.value)}
                rows={3}
                placeholder="Task 描述"
                className="agentdash-form-textarea"
              />
              <select
                value={editStatus}
                onChange={(event) => setEditStatus(event.target.value as TaskStatus)}
                className="agentdash-form-select"
              >
                <option value="pending">待执行</option>
                <option value="assigned">已分配</option>
                <option value="running">执行中</option>
                <option value="awaiting_verification">待验收</option>
                <option value="completed">已完成</option>
                <option value="failed">失败</option>
              </select>

              <select
                value={editWorkspaceId}
                onChange={(event) => setEditWorkspaceId(event.target.value)}
                className="agentdash-form-select"
              >
                <option value="">不绑定 Workspace</option>
                {workspaces.map((workspace) => (
                  <option key={workspace.id} value={workspace.id}>
                    {workspace.name}
                  </option>
                ))}
              </select>

              <div className="rounded-[12px] border border-border bg-background p-3">
                <p className="mb-2 text-xs text-muted-foreground">Agent 绑定</p>
                <AgentBindingFields
                  value={editAgentBinding}
                  projectConfig={projectConfig}
                  onChange={setEditAgentBinding}
                />
              </div>
              <div className="flex justify-end">
                <button
                  type="button"
                  onClick={() => void handleSaveTask()}
                  className="agentdash-button-secondary"
                >
                  保存 Task
                </button>
              </div>
            </DetailSection>

            {(formMessage || error) && (
              <p className="text-xs text-destructive">{formMessage || error}</p>
            )}
          </div>

          <div className="space-y-4">
            <DetailSection
              title="Agent 执行会话"
              description="不跳转页面，直接在抽屉中查看实时输出和进度。"
            >
              <TaskAgentSessionPanel
                task={task}
                onTaskUpdated={(updated) => {
                  applyTaskSnapshot(updated);
                  onTaskUpdated(updated);
                }}
              />
            </DetailSection>

            <DetailSection
              title="执行产物"
              extra={
                hasArtifacts ? (
                  <button
                    type="button"
                    onClick={() => setIsArtifactsCollapsed((value) => !value)}
                    aria-expanded={!isArtifactsCollapsed}
                    className="rounded-[8px] border border-border bg-background px-2 py-1 text-xs text-muted-foreground transition-colors hover:bg-secondary"
                  >
                    {isArtifactsCollapsed ? `展开（${sortedArtifacts.length}）` : "收起"}
                  </button>
                ) : null
              }
            >
              {!hasArtifacts ? (
                <p className="rounded-[12px] border border-dashed border-border bg-secondary/25 px-3 py-6 text-center text-sm text-muted-foreground">
                  暂无执行产物
                </p>
              ) : isArtifactsCollapsed ? (
                <p className="rounded-[12px] border border-dashed border-border bg-secondary/25 px-3 py-6 text-center text-sm text-muted-foreground">
                  已折叠执行产物，点击右上角展开（共 {sortedArtifacts.length} 条）
                </p>
              ) : (
                <div className="space-y-2">
                  {sortedArtifacts.map((artifact) => (
                    <ArtifactBlock key={artifact.id} artifact={artifact} />
                  ))}
                </div>
              )}
            </DetailSection>
          </div>
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
