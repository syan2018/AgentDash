import { useEffect, useMemo, useState } from "react";
import { useNavigate, useParams } from "react-router-dom";
import type { AgentBinding, ProjectConfig, Story, StoryStatus, Task, Workspace } from "../types";
import { StoryStatusBadge } from "../components/ui/status-badge";
import { TaskList } from "../features/task/task-list";
import { TaskDrawer } from "../features/task/task-drawer";
import { AgentBindingFields } from "../features/task/agent-binding-fields";
import {
  createDefaultAgentBinding,
  hasAgentBindingSelection,
  normalizeAgentBinding,
  resolveDefaultWorkspaceId,
} from "../features/task/agent-binding";
import { useStoryStore } from "../stores/storyStore";
import { useProjectStore } from "../stores/projectStore";
import { useWorkspaceStore } from "../stores/workspaceStore";
import {
  DangerConfirmDialog,
  DetailMenu,
  DetailSection,
} from "../components/ui/detail-panel";

type TabKey = "context" | "tasks" | "review";

// 状态流转操作按钮组
interface StoryStatusActionsProps {
  currentStatus: StoryStatus;
  onStatusChange: (status: StoryStatus) => void;
}

function StoryStatusActions({ currentStatus, onStatusChange }: StoryStatusActionsProps) {
  // 根据当前状态定义可用的流转操作
  const getAvailableActions = (status: StoryStatus): Array<{ label: string; status: StoryStatus; variant: "primary" | "secondary" | "danger" }> => {
    switch (status) {
      case "draft":
        return [
          { label: "标记就绪", status: "ready", variant: "primary" },
          { label: "取消", status: "cancelled", variant: "danger" },
        ];
      case "ready":
        return [
          { label: "开始执行", status: "running", variant: "primary" },
          { label: "退回草稿", status: "draft", variant: "secondary" },
          { label: "取消", status: "cancelled", variant: "danger" },
        ];
      case "running":
        return [
          { label: "提交验收", status: "review", variant: "primary" },
          { label: "标记失败", status: "failed", variant: "danger" },
        ];
      case "review":
        return [
          { label: "验收通过", status: "completed", variant: "primary" },
          { label: "退回执行", status: "running", variant: "secondary" },
          { label: "验收不通过", status: "failed", variant: "danger" },
        ];
      case "completed":
        return [
          { label: "重新打开", status: "ready", variant: "secondary" },
        ];
      case "failed":
        return [
          { label: "重新执行", status: "running", variant: "primary" },
          { label: "关闭", status: "cancelled", variant: "secondary" },
        ];
      case "cancelled":
        return [
          { label: "重新打开", status: "draft", variant: "primary" },
        ];
      default:
        return [];
    }
  };

  const actions = getAvailableActions(currentStatus);

  if (actions.length === 0) return null;

  return (
    <DetailSection title="状态流转">
      <div className="flex flex-wrap gap-2">
        {actions.map((action) => {
          // 低饱和度配色
          const variantClasses = {
            primary: "bg-primary/10 text-primary hover:bg-primary/20 border border-primary/30",
            secondary: "bg-muted text-muted-foreground hover:bg-muted/80 border border-border",
            danger: "bg-destructive/10 text-destructive hover:bg-destructive/20 border border-destructive/30",
          };
          return (
            <button
              key={action.status}
              type="button"
              onClick={() => onStatusChange(action.status)}
              className={`rounded px-3 py-1.5 text-xs font-medium transition-colors ${variantClasses[action.variant]}`}
            >
              {action.label}
            </button>
          );
        })}
      </div>
      <div className="mt-2 flex items-center gap-2 text-xs text-muted-foreground">
        <span>当前状态:</span>
        <StoryStatusBadge status={currentStatus} />
      </div>
    </DetailSection>
  );
}

interface CreateTaskPanelProps {
  storyId: string;
  workspaces: Workspace[];
  projectConfig?: ProjectConfig;
  onCreated: () => void;
}

function CreateTaskPanel({
  storyId,
  workspaces,
  projectConfig,
  onCreated,
}: CreateTaskPanelProps) {
  const { createTask, error } = useStoryStore();
  const [isExpanded, setIsExpanded] = useState(false);
  const [title, setTitle] = useState("");
  const [description, setDescription] = useState("");
  const [workspaceId, setWorkspaceId] = useState(() => resolveDefaultWorkspaceId(projectConfig, workspaces));
  const [agentBinding, setAgentBinding] = useState<AgentBinding>(() => createDefaultAgentBinding(projectConfig));
  const [isSubmitting, setIsSubmitting] = useState(false);
  const [formMessage, setFormMessage] = useState<string | null>(null);

  useEffect(() => {
    if (isExpanded) return;
    setWorkspaceId(resolveDefaultWorkspaceId(projectConfig, workspaces));
    setAgentBinding(createDefaultAgentBinding(projectConfig));
    setFormMessage(null);
  }, [isExpanded, projectConfig, workspaces]);

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

      onCreated();
      // 重置表单并收起
      setTitle("");
      setDescription("");
      setWorkspaceId(resolveDefaultWorkspaceId(projectConfig, workspaces));
      setAgentBinding(createDefaultAgentBinding(projectConfig));
      setIsExpanded(false);
    } finally {
      setIsSubmitting(false);
    }
  };

  if (!isExpanded) {
    return (
      <button
        type="button"
        onClick={() => setIsExpanded(true)}
        className="flex w-full items-center justify-center gap-2 rounded-lg border border-dashed border-border bg-card py-3 text-sm text-muted-foreground transition-colors hover:border-primary hover:text-primary"
      >
        <svg className="h-4 w-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
          <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 4v16m8-8H4" />
        </svg>
        添加 Task
      </button>
    );
  }

  return (
    <div className="rounded-lg border border-border bg-card p-4">
      <div className="mb-3 flex items-center justify-between">
        <span className="text-sm font-medium">新建 Task</span>
        <button
          type="button"
          onClick={() => setIsExpanded(false)}
          className="text-xs text-muted-foreground hover:text-foreground"
        >
          取消
        </button>
      </div>

      <div className="space-y-3">
        <input
          value={title}
          onChange={(event) => setTitle(event.target.value)}
          placeholder="Task 标题"
          autoFocus
          className="w-full rounded border border-border bg-background px-3 py-2 text-sm outline-none ring-ring focus:ring-1"
        />

        <select
          value={workspaceId}
          onChange={(event) => setWorkspaceId(event.target.value)}
          className="w-full rounded border border-border bg-background px-3 py-2 text-sm outline-none ring-ring focus:ring-1"
        >
          <option value="">Workspace</option>
          {workspaces.map((workspace) => (
            <option key={workspace.id} value={workspace.id}>
              {workspace.name}
            </option>
          ))}
        </select>

        <textarea
          value={description}
          onChange={(event) => setDescription(event.target.value)}
          rows={2}
          placeholder="描述（可选）"
          className="w-full rounded border border-border bg-background px-3 py-2 text-sm outline-none ring-ring focus:ring-1"
        />

        <AgentBindingFields
          value={agentBinding}
          projectConfig={projectConfig}
          onChange={setAgentBinding}
        />

        <div className="flex items-center justify-between">
          {formMessage || error ? (
            <p className="text-xs text-destructive">{formMessage || error}</p>
          ) : (
            <div />
          )}
          <button
            type="button"
            onClick={() => void handleSubmit()}
            disabled={isSubmitting || !title.trim()}
            className="rounded bg-primary px-4 py-1.5 text-sm text-primary-foreground disabled:opacity-50"
          >
            {isSubmitting ? "创建中..." : "创建"}
          </button>
        </div>
      </div>
    </div>
  );
}

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
        <div className="space-y-4">
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
        </div>
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

export function StoryPage() {
  const { storyId } = useParams<{ storyId: string }>();
  const navigate = useNavigate();
  const { projects } = useProjectStore();
  const { stories, tasksByStoryId, fetchStoriesByProject, fetchTasks, updateStory, deleteStory, error } = useStoryStore();
  const { workspacesByProjectId } = useWorkspaceStore();

  const [activeTab, setActiveTab] = useState<TabKey>("context");
  const [isDeleteConfirmOpen, setIsDeleteConfirmOpen] = useState(false);
  const [deleteConfirmValue, setDeleteConfirmValue] = useState("");
  const [formMessage, setFormMessage] = useState<string | null>(null);
  const [isEditingBasicInfo, setIsEditingBasicInfo] = useState(false);
  const [selectedTaskId, setSelectedTaskId] = useState<string | null>(null);

  // 获取当前 Story
  const story = useMemo(() => stories.find((s) => s.id === storyId) || null, [stories, storyId]);

  // 编辑表单状态 - 使用 key 模式在 storyId 变化时重置
  const [editTitle, setEditTitle] = useState(story?.title ?? "");
  const [editDescription, setEditDescription] = useState(story?.description ?? "");
  const [editStatus, setEditStatus] = useState<StoryStatus>(story?.status ?? "draft");

  // 当 storyId 变化时重置表单（通过 key 属性实现，这里作为备份）
  useEffect(() => {
    if (story) {
      setEditTitle(story.title);
      setEditDescription(story.description || "");
      setEditStatus(story.status);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [storyId]);

  // 获取 Story 相关数据
  const tasks = useMemo(() => (storyId ? tasksByStoryId[storyId] ?? [] : []), [tasksByStoryId, storyId]);
  const sortedTasks = useMemo(
    () => [...tasks].sort((a, b) => new Date(b.updated_at).getTime() - new Date(a.updated_at).getTime()),
    [tasks]
  );
  const selectedTask = useMemo(
    () => sortedTasks.find((task) => task.id === selectedTaskId) ?? null,
    [sortedTasks, selectedTaskId]
  );

  const currentProject = useMemo(() => {
    if (!story) return null;
    return projects.find((p) => p.id === story.project_id) || null;
  }, [story, projects]);

  const workspaces = useMemo(() => {
    if (!story) return [];
    return workspacesByProjectId[story.project_id] ?? [];
  }, [story, workspacesByProjectId]);

  // 加载 Story 数据
  useEffect(() => {
    if (!story && storyId) {
      // Story 不在列表中，可能需要先加载项目数据
      // 这里简化处理，实际可能需要根据 storyId 反查 projectId
      const loadStory = async () => {
        // 尝试从已有的项目中查找
        for (const project of projects) {
          await fetchStoriesByProject(project.id);
        }
      };
      void loadStory();
    }
  }, [story, storyId, projects, fetchStoriesByProject]);

  // 加载 Tasks
  useEffect(() => {
    if (storyId && !tasksByStoryId[storyId]) {
      void fetchTasks(storyId);
    }
  }, [storyId, tasksByStoryId, fetchTasks]);


  const handleSaveStory = async () => {
    if (!story) return;
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
    setIsEditingBasicInfo(false);
  };

  const handleDeleteStory = async () => {
    if (!story) return;
    if (deleteConfirmValue.trim() !== story.title) {
      setFormMessage("请输入完整 Story 标题后再删除");
      return;
    }
    await deleteStory(story.id);
    setIsDeleteConfirmOpen(false);
    navigate("/");
  };

  const handleTaskCreated = () => {
    // Task 创建成功后刷新列表
    if (storyId) {
      void fetchTasks(storyId);
    }
  };

  const handleTaskUpdated = (updated: Task) => {
    setSelectedTaskId(updated.id);
    if (storyId) {
      void fetchTasks(storyId);
    }
  };

  const handleTaskDeleted = (taskId: string, _storyId: string) => {
    if (selectedTaskId === taskId) {
      setSelectedTaskId(null);
    }
    if (storyId) {
      void fetchTasks(storyId);
    }
  };

  if (!story) {
    return (
      <div className="flex h-full items-center justify-center">
        <div className="text-center">
          <h2 className="text-xl font-semibold text-foreground">Story 不存在</h2>
          <p className="mt-2 text-sm text-muted-foreground">该 Story 可能已被删除或无法访问</p>
          <button
            type="button"
            onClick={() => navigate("/")}
            className="mt-4 rounded bg-primary px-4 py-2 text-sm text-primary-foreground"
          >
            返回看板
          </button>
        </div>
      </div>
    );
  }

  const tabs = [
    { key: "context", label: "上下文" },
    { key: "tasks", label: "任务列表" },
    { key: "review", label: "验收" },
  ] as const;

  return (
    <div className="flex h-full flex-col overflow-hidden">
      {/* 页面头部 */}
      <header className="flex h-14 shrink-0 items-center justify-between border-b border-border bg-card px-6">
        <div className="flex items-center gap-4">
          <button
            type="button"
            onClick={() => navigate("/")}
            className="text-sm text-muted-foreground hover:text-foreground"
          >
            ← 返回看板
          </button>
          <div className="h-4 w-px bg-border" />
          <div>
            <h1 className="text-sm font-semibold text-foreground">{story.title}</h1>
            <p className="text-xs text-muted-foreground">ID: {story.id}</p>
          </div>
        </div>
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
      </header>

      {/* 页面内容 */}
      <div className="flex flex-1 overflow-hidden">
        {/* 左侧：Story 编辑 */}
        <div className="w-80 shrink-0 overflow-y-auto border-r border-border bg-card p-4">
          {/* Story 基本信息 */}
          <DetailSection
            title="基本信息"
            extra={
              !isEditingBasicInfo && (
                <button
                  type="button"
                  onClick={() => setIsEditingBasicInfo(true)}
                  className="text-xs text-muted-foreground hover:text-foreground"
                >
                  编辑
                </button>
              )
            }
          >
            {isEditingBasicInfo ? (
              <div className="space-y-3">
                <div>
                  <label className="mb-1 block text-xs text-muted-foreground">标题</label>
                  <input
                    value={editTitle}
                    onChange={(event) => setEditTitle(event.target.value)}
                    placeholder="Story 标题"
                    autoFocus
                    className="w-full rounded border border-border bg-background px-2 py-1.5 text-sm outline-none ring-ring focus:ring-1"
                  />
                </div>
                <div>
                  <label className="mb-1 block text-xs text-muted-foreground">描述</label>
                  <textarea
                    value={editDescription}
                    onChange={(event) => setEditDescription(event.target.value)}
                    rows={3}
                    placeholder="Story 描述"
                    className="w-full rounded border border-border bg-background px-2 py-1.5 text-sm outline-none ring-ring focus:ring-1"
                  />
                </div>
                <div className="flex gap-2">
                  <button
                    type="button"
                    onClick={() => {
                      setIsEditingBasicInfo(false);
                      // 重置为原始值
                      if (story) {
                        setEditTitle(story.title);
                        setEditDescription(story.description || "");
                      }
                    }}
                    className="flex-1 rounded border border-border bg-background px-3 py-1.5 text-sm hover:bg-muted"
                  >
                    取消
                  </button>
                  <button
                    type="button"
                    onClick={() => void handleSaveStory()}
                    className="flex-1 rounded bg-primary px-3 py-1.5 text-sm text-primary-foreground hover:bg-primary/90"
                  >
                    保存
                  </button>
                </div>
              </div>
            ) : (
              <div className="space-y-2">
                <div>
                  <span className="text-xs text-muted-foreground">标题</span>
                  <p className="mt-0.5 text-sm font-medium">{story.title}</p>
                </div>
                <div>
                  <span className="text-xs text-muted-foreground">描述</span>
                  <p className="mt-0.5 text-sm text-foreground">
                    {story.description || <span className="text-muted-foreground">暂无描述</span>}
                  </p>
                </div>
              </div>
            )}
          </DetailSection>

          {/* 状态流转操作 */}
          <div className="mt-4">
            <StoryStatusActions
              currentStatus={story.status}
              onStatusChange={(status) => void updateStory(story.id, { status })}
            />
          </div>

          {/* 创建 Task */}
          <div className="mt-4">
            <CreateTaskPanel
              storyId={story.id}
              workspaces={workspaces}
              projectConfig={currentProject?.config}
              onCreated={handleTaskCreated}
            />
          </div>

          {(formMessage || error) && <p className="mt-4 text-xs text-destructive">{formMessage || error}</p>}
        </div>

        {/* 右侧：Tab 内容 */}
        <div className="flex flex-1 flex-col overflow-hidden bg-background">
          {/* Tab 导航 */}
          <div className="flex border-b border-border bg-card">
            {tabs.map((tab) => (
              <button
                key={tab.key}
                type="button"
                onClick={() => setActiveTab(tab.key)}
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

          {/* Tab 内容 */}
          <div className="flex-1 overflow-y-auto p-6">
            {activeTab === "context" && <ContextPanel story={story} />}
            {activeTab === "tasks" && (
              <DetailSection title="任务列表">
                <TaskList tasks={sortedTasks} onTaskClick={(task) => setSelectedTaskId(task.id)} />
              </DetailSection>
            )}
            {activeTab === "review" && <ReviewPanel story={story} tasks={sortedTasks} />}
          </div>
        </div>
      </div>

      <TaskDrawer
        task={selectedTask}
        workspaces={workspaces}
        projectConfig={currentProject?.config}
        onTaskUpdated={handleTaskUpdated}
        onTaskDeleted={handleTaskDeleted}
        onClose={() => setSelectedTaskId(null)}
      />

      {/* 删除确认对话框 */}
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
    </div>
  );
}
