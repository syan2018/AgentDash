import { useEffect, useMemo, useState } from "react";
import type { AgentBinding, ProjectConfig, Story, StoryStatus, StoryPriority, StoryType, Task, Workspace } from "../../types";
import { StoryStatusBadge, StoryPriorityBadge, StoryTypeBadge, getStoryTypeInfo, getStoryPriorityInfo } from "../../components/ui/status-badge";
import { TaskList } from "../task/task-list";
import { AgentBindingFields } from "../task/agent-binding-fields";
import {
  createDefaultAgentBinding,
  hasAgentBindingSelection,
  normalizeAgentBinding,
  resolveDefaultWorkspaceId,
} from "../task/agent-binding";
import { useStoryStore } from "../../stores/storyStore";
import {
  DangerConfirmDialog,
  DetailMenu,
  DetailPanel,
  DetailSection,
} from "../../components/ui/detail-panel";

// Story 优先级选项
const priorityOptions: { value: StoryPriority; label: string; description: string }[] = [
  { value: "p0", label: "P0 - 紧急", description: "需要立即处理，阻塞其他工作" },
  { value: "p1", label: "P1 - 高", description: "重要功能，应尽快完成" },
  { value: "p2", label: "P2 - 中", description: "正常优先级，按计划进行" },
  { value: "p3", label: "P3 - 低", description: "可以延后处理" },
];

// Story 类型选项
const storyTypeOptions: { value: StoryType; label: string; icon: string; description: string }[] = [
  { value: "feature", label: "功能", icon: "✨", description: "新功能开发" },
  { value: "bugfix", label: "缺陷", icon: "🐛", description: "Bug 修复" },
  { value: "refactor", label: "重构", icon: "♻️", description: "代码重构" },
  { value: "docs", label: "文档", icon: "📝", description: "文档更新" },
  { value: "test", label: "测试", icon: "🧪", description: "测试相关" },
  { value: "other", label: "其他", icon: "📦", description: "其他类型" },
];

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
  const runningCount = tasks.filter((task) => task.status === "running").length;
  const pendingCount = tasks.filter((task) => task.status === "pending" || task.status === "assigned").length;

  const typeInfo = getStoryTypeInfo(story.story_type);
  const priorityInfo = getStoryPriorityInfo(story.priority);

  return (
    <DetailSection title="验收">
      {/* Story 元信息卡片 */}
      <div className="mb-4 grid grid-cols-2 gap-3 sm:grid-cols-4">
        <div className="rounded-md border border-border bg-card p-3">
          <p className="text-xs text-muted-foreground">类型</p>
          <div className="mt-1 flex items-center gap-1.5">
            <span className="text-base">{typeInfo.icon}</span>
            <span className="text-sm font-medium text-foreground">{typeInfo.label}</span>
          </div>
        </div>
        <div className="rounded-md border border-border bg-card p-3">
          <p className="text-xs text-muted-foreground">优先级</p>
          <div className="mt-1 flex items-center gap-1.5">
            <span className={`inline-block h-2 w-2 rounded-full ${priorityInfo.dotColor}`} />
            <span className="text-sm font-medium text-foreground">{priorityInfo.label}</span>
          </div>
        </div>
        <div className="rounded-md border border-border bg-card p-3">
          <p className="text-xs text-muted-foreground">状态</p>
          <div className="mt-1">
            <StoryStatusBadge status={story.status} />
          </div>
        </div>
        <div className="rounded-md border border-border bg-card p-3">
          <p className="text-xs text-muted-foreground">任务总数</p>
          <p className="mt-1 text-sm font-medium text-foreground">{tasks.length}</p>
        </div>
      </div>

      {/* 标签 */}
      {story.tags.length > 0 && (
        <div className="mb-4">
          <p className="mb-2 text-xs text-muted-foreground">标签</p>
          <div className="flex flex-wrap gap-1.5">
            {story.tags.map((tag, index) => (
              <span
                key={index}
                className="inline-flex items-center rounded bg-secondary px-2 py-1 text-xs text-secondary-foreground"
              >
                {tag}
              </span>
            ))}
          </div>
        </div>
      )}

      {/* 任务统计 */}
      <div className="mb-4 grid grid-cols-2 gap-3 sm:grid-cols-4">
        <div className="rounded-md border border-border bg-card p-3">
          <p className="text-xs text-muted-foreground">待执行</p>
          <p className="mt-1 text-sm font-medium text-muted-foreground">{pendingCount}</p>
        </div>
        <div className="rounded-md border border-border bg-card p-3">
          <p className="text-xs text-muted-foreground">执行中</p>
          <p className="mt-1 text-sm font-medium text-primary">{runningCount}</p>
        </div>
        <div className="rounded-md border border-border bg-card p-3">
          <p className="text-xs text-muted-foreground">成功</p>
          <p className="mt-1 text-sm font-medium text-success">{successCount}</p>
        </div>
        <div className="rounded-md border border-border bg-card p-3">
          <p className="text-xs text-muted-foreground">失败</p>
          <p className="mt-1 text-sm font-medium text-destructive">{failedCount}</p>
        </div>
      </div>

      {/* 描述 */}
      <div className="rounded-md border border-border bg-card p-3">
        <p className="mb-2 text-xs text-muted-foreground">描述</p>
        <p className="text-sm text-foreground">{story.description || "暂无 Story 描述"}</p>
      </div>
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
  const [workspaceId, setWorkspaceId] = useState(() => resolveDefaultWorkspaceId(projectConfig, workspaces));
  const [agentBinding, setAgentBinding] = useState<AgentBinding>(() => createDefaultAgentBinding(projectConfig));
  const [isSubmitting, setIsSubmitting] = useState(false);
  const [formMessage, setFormMessage] = useState<string | null>(null);

  useEffect(() => {
    if (!open) return;
    setWorkspaceId(resolveDefaultWorkspaceId(projectConfig, workspaces));
    setAgentBinding(createDefaultAgentBinding(projectConfig));
    setFormMessage(null);
  }, [open, projectConfig, workspaces]);

  const handleSubmit = async () => {
    if (!title.trim()) return;
    if (!hasAgentBindingSelection(agentBinding, projectConfig)) {
      setFormMessage("请指定 Agent 类型或预设，或先在 Project 配置中设置 default_agent_type");
      return;
    }
    setIsSubmitting(true);
    setFormMessage(null);
    try {
      const task = await createTask(storyId, {
        title: title.trim(),
        description: description.trim() || undefined,
        workspace_id: workspaceId || null,
        agent_binding: normalizeAgentBinding(agentBinding),
      });
      if (!task) return;

      setTitle("");
      setDescription("");
      setWorkspaceId(resolveDefaultWorkspaceId(projectConfig, workspaces));
      setAgentBinding(createDefaultAgentBinding(projectConfig));
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
          <AgentBindingFields
            value={agentBinding}
            projectConfig={projectConfig}
            onChange={setAgentBinding}
          />
        </DetailSection>

        {(formMessage || error) && <p className="text-xs text-destructive">创建失败：{formMessage || error}</p>}

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
  const [editPriority, setEditPriority] = useState<StoryPriority>(story?.priority ?? "p2");
  const [editStoryType, setEditStoryType] = useState<StoryType>(story?.story_type ?? "feature");
  const [editTags, setEditTags] = useState<string>(story?.tags.join(", ") ?? "");
  const [formMessage, setFormMessage] = useState<string | null>(null);
  const [isDeleteConfirmOpen, setIsDeleteConfirmOpen] = useState(false);
  const [deleteConfirmValue, setDeleteConfirmValue] = useState("");

  // 当 story 变化时同步编辑状态
  useEffect(() => {
    if (story) {
      setEditTitle(story.title);
      setEditDescription(story.description ?? "");
      setEditStatus(story.status);
      setEditPriority(story.priority);
      setEditStoryType(story.story_type);
      setEditTags(story.tags.join(", "));
    }
  }, [story?.id]);

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

    // 解析标签
    const parsedTags = editTags
      .split(",")
      .map((t) => t.trim())
      .filter((t) => t.length > 0);

    const updated = await updateStory(story.id, {
      title: trimmedTitle,
      description: editDescription,
      status: editStatus,
      priority: editPriority,
      story_type: editStoryType,
      tags: parsedTags,
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
            {/* 标题 */}
            <input
              value={editTitle}
              onChange={(event) => setEditTitle(event.target.value)}
              placeholder="Story 标题"
              className="w-full rounded border border-border bg-background px-2 py-1.5 text-sm outline-none ring-ring focus:ring-1"
            />

            {/* 描述 */}
            <textarea
              value={editDescription}
              onChange={(event) => setEditDescription(event.target.value)}
              rows={3}
              placeholder="Story 描述"
              className="w-full rounded border border-border bg-background px-2 py-1.5 text-sm outline-none ring-ring focus:ring-1"
            />

            {/* 状态、优先级、类型 三列布局 */}
            <div className="grid grid-cols-3 gap-3">
              <div>
                <label className="mb-1 block text-xs text-muted-foreground">状态</label>
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
              </div>

              <div>
                <label className="mb-1 block text-xs text-muted-foreground">优先级</label>
                <select
                  value={editPriority}
                  onChange={(event) => setEditPriority(event.target.value as StoryPriority)}
                  className="w-full rounded border border-border bg-background px-2 py-1.5 text-sm outline-none ring-ring focus:ring-1"
                >
                  {priorityOptions.map((opt) => (
                    <option key={opt.value} value={opt.value}>
                      {opt.label}
                    </option>
                  ))}
                </select>
              </div>

              <div>
                <label className="mb-1 block text-xs text-muted-foreground">类型</label>
                <select
                  value={editStoryType}
                  onChange={(event) => setEditStoryType(event.target.value as StoryType)}
                  className="w-full rounded border border-border bg-background px-2 py-1.5 text-sm outline-none ring-ring focus:ring-1"
                >
                  {storyTypeOptions.map((opt) => (
                    <option key={opt.value} value={opt.value}>
                      {opt.icon} {opt.label}
                    </option>
                  ))}
                </select>
              </div>
            </div>

            {/* 标签 */}
            <div>
              <label className="mb-1 block text-xs text-muted-foreground">标签（用逗号分隔）</label>
              <input
                value={editTags}
                onChange={(event) => setEditTags(event.target.value)}
                placeholder="例如: frontend, api, urgent"
                className="w-full rounded border border-border bg-background px-2 py-1.5 text-sm outline-none ring-ring focus:ring-1"
              />
              {/* 标签预览 */}
              <div className="mt-2 flex flex-wrap gap-1">
                {editTags
                  .split(",")
                  .map((t) => t.trim())
                  .filter((t) => t.length > 0)
                  .map((tag, index) => (
                    <span
                      key={index}
                      className="inline-flex items-center rounded bg-secondary px-2 py-0.5 text-xs text-secondary-foreground"
                    >
                      {tag}
                    </span>
                  ))}
              </div>
            </div>

            {/* 当前值展示 */}
            <div className="flex items-center gap-2 rounded bg-muted/50 px-3 py-2">
              <span className="text-xs text-muted-foreground">当前:</span>
              <StoryTypeBadge type={editStoryType} />
              <StoryPriorityBadge priority={editPriority} showLabel />
              <StoryStatusBadge status={editStatus} />
            </div>

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
