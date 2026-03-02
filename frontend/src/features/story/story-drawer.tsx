import { useMemo, useState } from "react";
import type { ProjectConfig, Story, StoryStatus, Task, Workspace } from "../../types";
import { StoryStatusBadge } from "../../components/ui/status-badge";
import { TaskList } from "../task/task-list";
import { useStoryStore } from "../../stores/storyStore";
import {
  DangerConfirmDialog,
  DetailMenu,
  DetailPanel,
  DetailSection,
} from "../../components/ui/detail-panel";

interface StoryDrawerProps {
  story: Story | null;
  tasks: Task[];
  workspaces: Workspace[];
  projectConfig?: ProjectConfig;
  onClose: () => void;
  onStoryUpdated: (story: Story) => void;
  onStoryDeleted: (storyId: string) => void;
  onOpenTask: (task: Task) => void;
}

type DrawerTab = "context" | "tasks" | "review";

function ContextPanel({ story }: { story: Story }) {
  const ctx = story.context;
  const hasContent = ctx.prd_doc || ctx.spec_refs.length > 0 || ctx.resource_list.length > 0;

  return (
    <DetailSection title="上下文">
      {!hasContent ? (
        <p className="rounded-md border border-dashed border-border px-3 py-6 text-center text-sm text-muted-foreground">
          暂无上下文条目
        </p>
      ) : (
        <>
          {ctx.prd_doc && (
            <div className="rounded-md border border-border bg-card p-3">
              <p className="mb-1 text-xs font-medium text-muted-foreground">PRD 文档</p>
              <pre className="whitespace-pre-wrap text-sm text-foreground">{ctx.prd_doc}</pre>
            </div>
          )}

          {ctx.spec_refs.length > 0 && (
            <div className="rounded-md border border-border bg-card p-3">
              <p className="mb-2 text-xs font-medium text-muted-foreground">规格引用</p>
              <ul className="space-y-1">
                {ctx.spec_refs.map((ref, index) => (
                  <li key={index} className="text-sm text-foreground">
                    <span className="mr-2 text-muted-foreground">·</span>
                    {ref}
                  </li>
                ))}
              </ul>
            </div>
          )}

          {ctx.resource_list.length > 0 && (
            <div className="rounded-md border border-border bg-card p-3">
              <p className="mb-2 text-xs font-medium text-muted-foreground">资源列表</p>
              {ctx.resource_list.map((resource, index) => (
                <div key={index} className="mb-1 flex items-center gap-2">
                  <span className="rounded bg-secondary px-2 py-0.5 text-[10px] uppercase text-muted-foreground">
                    {resource.resource_type}
                  </span>
                  <span className="text-sm text-foreground">{resource.name}</span>
                  <span className="text-xs text-muted-foreground">{resource.uri}</span>
                </div>
              ))}
            </div>
          )}
        </>
      )}
    </DetailSection>
  );
}

function ReviewPanel({ story, tasks }: { story: Story; tasks: Task[] }) {
  const successCount = tasks.filter((task) => task.status === "completed").length;
  const failedCount = tasks.filter((task) => task.status === "failed").length;

  return (
    <DetailSection title="验收">
      <div className="grid grid-cols-1 gap-3 sm:grid-cols-3">
        <div className="rounded-md border border-border bg-card p-3">
          <p className="text-xs text-muted-foreground">Story 状态</p>
          <p className="mt-1 text-sm font-medium text-foreground">{story.status}</p>
        </div>
        <div className="rounded-md border border-border bg-card p-3">
          <p className="text-xs text-muted-foreground">成功任务数</p>
          <p className="mt-1 text-sm font-medium text-success">{successCount}</p>
        </div>
        <div className="rounded-md border border-border bg-card p-3">
          <p className="text-xs text-muted-foreground">失败任务数</p>
          <p className="mt-1 text-sm font-medium text-destructive">{failedCount}</p>
        </div>
      </div>
      <p className="text-sm text-muted-foreground">{story.description || "暂无 Story 描述"}</p>
    </DetailSection>
  );
}

interface CreateTaskDrawerProps {
  open: boolean;
  storyId: string;
  workspaces: Workspace[];
  projectConfig?: ProjectConfig;
  onClose: () => void;
  onCreated: (task: Task) => void;
}

function CreateTaskDrawer({
  open,
  storyId,
  workspaces,
  projectConfig,
  onClose,
  onCreated,
}: CreateTaskDrawerProps) {
  const { createTask, error } = useStoryStore();
  const [title, setTitle] = useState("");
  const [description, setDescription] = useState("");
  const [workspaceId, setWorkspaceId] = useState(projectConfig?.default_workspace_id ?? "");
  const [agentType, setAgentType] = useState(projectConfig?.default_agent_type ?? "");
  const [presetName, setPresetName] = useState("");
  const [isSubmitting, setIsSubmitting] = useState(false);

  const handlePresetChange = (value: string) => {
    setPresetName(value);
    if (!value || !projectConfig) return;
    const preset = projectConfig.agent_presets.find((item) => item.name === value);
    if (preset) {
      setAgentType(preset.agent_type);
    }
  };

  const handleSubmit = async () => {
    if (!title.trim()) return;
    setIsSubmitting(true);
    try {
      const task = await createTask(storyId, {
        title: title.trim(),
        description: description.trim() || undefined,
        workspace_id: workspaceId || null,
        agent_binding: {
          agent_type: agentType.trim() || null,
          preset_name: presetName || null,
        },
      });
      if (!task) return;

      onCreated(task);
      onClose();
    } finally {
      setIsSubmitting(false);
    }
  };

  return (
    <DetailPanel
      open={open}
      title="创建 Task"
      subtitle="绑定 Workspace 与 Agent 后创建任务"
      onClose={onClose}
      widthClassName="max-w-3xl"
      overlayClassName="z-40"
      panelClassName="z-50"
    >
      <div className="space-y-4 p-5">
        <DetailSection title="任务信息">
          <input
            value={title}
            onChange={(event) => setTitle(event.target.value)}
            placeholder="Task 标题"
            className="w-full rounded border border-border bg-background px-2 py-1.5 text-sm outline-none ring-ring focus:ring-1"
          />
          <textarea
            value={description}
            onChange={(event) => setDescription(event.target.value)}
            rows={3}
            placeholder="描述（可选）"
            className="w-full rounded border border-border bg-background px-2 py-1.5 text-sm outline-none ring-ring focus:ring-1"
          />
          <select
            value={workspaceId}
            onChange={(event) => setWorkspaceId(event.target.value)}
            className="w-full rounded border border-border bg-background px-2 py-1.5 text-sm outline-none ring-ring focus:ring-1"
          >
            <option value="">不绑定 Workspace</option>
            {workspaces.map((workspace) => (
              <option key={workspace.id} value={workspace.id}>
                {workspace.name}
              </option>
            ))}
          </select>
        </DetailSection>

        <DetailSection title="Agent 绑定">
          <input
            value={agentType}
            onChange={(event) => setAgentType(event.target.value)}
            placeholder="Agent 类型（可选，留空则使用项目默认）"
            className="w-full rounded border border-border bg-background px-2 py-1.5 text-sm outline-none ring-ring focus:ring-1"
          />
          <select
            value={presetName}
            onChange={(event) => handlePresetChange(event.target.value)}
            className="w-full rounded border border-border bg-background px-2 py-1.5 text-sm outline-none ring-ring focus:ring-1"
          >
            <option value="">不使用预设</option>
            {(projectConfig?.agent_presets ?? []).map((preset) => (
              <option key={preset.name} value={preset.name}>
                {preset.name}
              </option>
            ))}
          </select>
        </DetailSection>

        {error && <p className="text-xs text-destructive">创建失败：{error}</p>}

        <div className="flex items-center justify-end border-t border-border pt-3">
          <button
            type="button"
            onClick={() => void handleSubmit()}
            disabled={isSubmitting || !title.trim()}
            className="rounded bg-primary px-3 py-1.5 text-sm text-primary-foreground disabled:opacity-50"
          >
            {isSubmitting ? "创建中..." : "创建 Task"}
          </button>
        </div>
      </div>
    </DetailPanel>
  );
}

export function StoryDrawer({
  story,
  tasks,
  workspaces,
  projectConfig,
  onClose,
  onStoryUpdated,
  onStoryDeleted,
  onOpenTask,
}: StoryDrawerProps) {
  const { updateStory, deleteStory, error } = useStoryStore();
  const [activeTab, setActiveTab] = useState<DrawerTab>("context");
  const [isCreateTaskOpen, setIsCreateTaskOpen] = useState(false);
  const [editTitle, setEditTitle] = useState(story?.title ?? "");
  const [editDescription, setEditDescription] = useState(story?.description ?? "");
  const [editStatus, setEditStatus] = useState<StoryStatus>(story?.status ?? "draft");
  const [formMessage, setFormMessage] = useState<string | null>(null);
  const [isDeleteConfirmOpen, setIsDeleteConfirmOpen] = useState(false);
  const [deleteConfirmValue, setDeleteConfirmValue] = useState("");

  const sortedTasks = useMemo(
    () =>
      [...tasks].sort(
        (a, b) => new Date(b.updated_at).getTime() - new Date(a.updated_at).getTime(),
      ),
    [tasks],
  );

  if (!story) return null;

  const handleSaveStory = async () => {
    const trimmedTitle = editTitle.trim();
    if (!trimmedTitle) {
      setFormMessage("Story 标题不能为空");
      return;
    }

    const updated = await updateStory(story.id, {
      title: trimmedTitle,
      description: editDescription,
      status: editStatus,
    });
    if (!updated) return;

    setFormMessage(null);
    onStoryUpdated(updated);
  };

  const handleDeleteStory = async () => {
    if (deleteConfirmValue.trim() !== story.title) {
      setFormMessage("请输入完整 Story 标题后再删除");
      return;
    }
    await deleteStory(story.id);
    setIsDeleteConfirmOpen(false);
    onStoryDeleted(story.id);
  };

  return (
    <>
      <DetailPanel
        open={Boolean(story)}
        title={story.title}
        subtitle={`ID: ${story.id}`}
        onClose={onClose}
        widthClassName="max-w-[80rem]"
        overlayClassName="z-20"
        panelClassName="z-30"
        headerExtra={
          <div className="flex items-center gap-2">
            <StoryStatusBadge status={story.status} />
            <DetailMenu
              items={[
                {
                  key: "delete",
                  label: "删除 Story",
                  danger: true,
                  onSelect: () => setIsDeleteConfirmOpen(true),
                },
              ]}
            />
          </div>
        }
      >
        <div className="space-y-4 p-5">
          <DetailSection title="Story 详情">
            <input
              value={editTitle}
              onChange={(event) => setEditTitle(event.target.value)}
              placeholder="Story 标题"
              className="w-full rounded border border-border bg-background px-2 py-1.5 text-sm outline-none ring-ring focus:ring-1"
            />
            <textarea
              value={editDescription}
              onChange={(event) => setEditDescription(event.target.value)}
              rows={3}
              placeholder="Story 描述"
              className="w-full rounded border border-border bg-background px-2 py-1.5 text-sm outline-none ring-ring focus:ring-1"
            />
            <select
              value={editStatus}
              onChange={(event) => setEditStatus(event.target.value as StoryStatus)}
              className="w-full rounded border border-border bg-background px-2 py-1.5 text-sm outline-none ring-ring focus:ring-1"
            >
              <option value="draft">草稿</option>
              <option value="ready">就绪</option>
              <option value="running">执行中</option>
              <option value="review">待验收</option>
              <option value="completed">已完成</option>
              <option value="failed">失败</option>
              <option value="cancelled">已取消</option>
            </select>
            <div className="flex items-center justify-end gap-2">
              <button
                type="button"
                onClick={() => void handleSaveStory()}
                className="rounded border border-border bg-secondary px-3 py-1.5 text-sm text-foreground hover:bg-secondary/70"
              >
                保存 Story
              </button>
            </div>
          </DetailSection>

          <div className="flex border-b border-border bg-card">
            {[
              { key: "context", label: "上下文" },
              { key: "tasks", label: "任务列表" },
              { key: "review", label: "验收" },
            ].map((tab) => (
              <button
                key={tab.key}
                type="button"
                onClick={() => setActiveTab(tab.key as DrawerTab)}
                className={`px-5 py-3 text-sm ${
                  activeTab === tab.key
                    ? "border-b-2 border-primary text-primary"
                    : "text-muted-foreground hover:text-foreground"
                }`}
              >
                {tab.label}
              </button>
            ))}
          </div>

          {activeTab === "context" && <ContextPanel story={story} />}

          {activeTab === "tasks" && (
            <DetailSection title="任务列表">
              <div className="mb-2 flex items-center justify-end">
                <button
                  type="button"
                  onClick={() => setIsCreateTaskOpen(true)}
                  className="rounded bg-primary px-3 py-1.5 text-sm text-primary-foreground"
                >
                  + 新建 Task
                </button>
              </div>
              <TaskList tasks={sortedTasks} onTaskClick={onOpenTask} />
            </DetailSection>
          )}

          {activeTab === "review" && <ReviewPanel story={story} tasks={sortedTasks} />}

          {(formMessage || error) && (
            <p className="text-xs text-destructive">{formMessage || error}</p>
          )}
        </div>
      </DetailPanel>

      <CreateTaskDrawer
        key={`story-create-task-${isCreateTaskOpen ? "open" : "closed"}-${story.id}`}
        open={isCreateTaskOpen}
        storyId={story.id}
        workspaces={workspaces}
        projectConfig={projectConfig}
        onClose={() => setIsCreateTaskOpen(false)}
        onCreated={onOpenTask}
      />

      <DangerConfirmDialog
        open={isDeleteConfirmOpen}
        title="删除 Story"
        description="Story 删除后其下 Task 会一起删除。"
        expectedValue={story.title}
        inputValue={deleteConfirmValue}
        onInputValueChange={setDeleteConfirmValue}
        confirmLabel="确认删除"
        onClose={() => {
          setIsDeleteConfirmOpen(false);
          setDeleteConfirmValue("");
        }}
        onConfirm={() => void handleDeleteStory()}
      />
    </>
  );
}
