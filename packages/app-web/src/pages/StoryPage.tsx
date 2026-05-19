import { useCallback, useEffect, useMemo, useState, type ReactNode } from "react";
import { useLocation, useNavigate, useParams } from "react-router-dom";
import type {
  ContextSourceKind,
  Story,
  StoryNavigationState,
  StoryPriority,
  StoryStatus,
  StoryType,
  Task,
  Workspace,
} from "../types";
import { StorySessionPanel } from "../features/story/story-session-panel";
import { StoryPriorityBadge, StoryStatusBadge, StoryTypeBadge, TaskStatusBadge } from "../components/ui/status-badge";
import { TaskDrawer } from "../features/task/task-drawer";
import { useStoryStore, findStoryById } from "../stores/storyStore";
import { useProjectStore } from "../stores/projectStore";
import { useWorkspaceStore } from "../stores/workspaceStore";
import {
  Badge,
  Button,
  DangerConfirmDialog,
  DetailMenu,
  EmptyState,
  Field,
  InspectorRow,
  SectionTitle,
  Select,
  Textarea,
  TextInput,
} from "@agentdash/ui";
import { CreateTaskPanel } from "../features/story/create-task-panel";
import { sourceKindMeta } from "../features/story/context-source-utils";
import { ContextPanel } from "../features/story/story-detail-panels";

const priorityOptions: { value: StoryPriority; label: string }[] = [
  { value: "p0", label: "P0 - 紧急" },
  { value: "p1", label: "P1 - 高" },
  { value: "p2", label: "P2 - 中" },
  { value: "p3", label: "P3 - 低" },
];

const storyTypeOptions: { value: StoryType; label: string }[] = [
  { value: "feature", label: "功能" },
  { value: "bugfix", label: "缺陷" },
  { value: "refactor", label: "重构" },
  { value: "docs", label: "文档" },
  { value: "test", label: "测试" },
  { value: "other", label: "其他" },
];

function contextSummary(sourceRefs: { kind: ContextSourceKind }[]) {
  const counts = new Map<ContextSourceKind, number>();
  for (const ref of sourceRefs) {
    counts.set(ref.kind, (counts.get(ref.kind) ?? 0) + 1);
  }
  return counts;
}

function contextSignalCount(story: Story): number {
  return (
    story.context.source_refs.length +
    story.context.context_containers.length +
    story.context.disabled_container_ids.length +
    (story.context.session_composition ? 1 : 0)
  );
}

function formatDate(value: string): string {
  return new Date(value).toLocaleDateString("zh-CN");
}

function taskReviewLabel(status: Task["status"]): { label: string; className: string } {
  switch (status) {
    case "awaiting_verification":
      return { label: "待验收", className: "text-warning" };
    case "completed":
      return { label: "通过", className: "text-success" };
    case "failed":
      return { label: "未通过", className: "text-destructive" };
    case "running":
      return { label: "执行中", className: "text-primary" };
    case "assigned":
    case "pending":
    default:
      return { label: "未开始", className: "text-muted-foreground" };
  }
}

function CompactProperty({
  children,
  className = "",
  label,
}: {
  children: ReactNode;
  className?: string;
  label: string;
}) {
  return (
    <div className={`min-w-0 space-y-1 ${className}`}>
      <p className="text-[10px] font-medium text-muted-foreground">{label}</p>
      <div className="min-w-0 text-xs text-foreground">{children}</div>
    </div>
  );
}

function BackToStoriesIcon() {
  return (
    <svg
      className="h-3.5 w-3.5"
      fill="none"
      viewBox="0 0 24 24"
      stroke="currentColor"
      strokeLinecap="round"
      strokeLinejoin="round"
      strokeWidth={2}
    >
      <path d="m15 18-6-6 6-6" />
    </svg>
  );
}

function isStoryNavigationState(value: unknown): value is StoryNavigationState {
  return Boolean(value && typeof value === "object" && (!("open_task_id" in value) || typeof value.open_task_id === "string"));
}

interface StoryStatusActionsProps {
  currentStatus: StoryStatus;
  onStatusChange: (status: StoryStatus) => void;
}

function StoryStatusActions({ currentStatus, onStatusChange }: StoryStatusActionsProps) {
  const actions = useMemo((): Array<{ label: string; status: StoryStatus; variant: "primary" | "secondary" | "danger" }> => {
    switch (currentStatus) {
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
        return [{ label: "重新打开", status: "ready", variant: "secondary" }];
      case "failed":
        return [
          { label: "重新执行", status: "running", variant: "primary" },
          { label: "关闭", status: "cancelled", variant: "secondary" },
        ];
      case "cancelled":
        return [{ label: "重新打开", status: "draft", variant: "primary" }];
      default:
        return [];
    }
  }, [currentStatus]);

  if (actions.length === 0) return null;

  return (
    <div className="flex flex-wrap gap-2">
      {actions.map((action) => (
        <Button
          key={action.status}
          type="button"
          size="sm"
          variant={action.variant}
          onClick={() => onStatusChange(action.status)}
        >
          {action.label}
        </Button>
      ))}
    </div>
  );
}

function StoryTaskRows({
  tasks,
  workspaces,
  onOpenTask,
}: {
  tasks: Task[];
  workspaces: Workspace[];
  onOpenTask: (task: Task) => void;
}) {
  if (tasks.length === 0) {
    return (
      <EmptyState className="py-8">
        当前 Story 暂无 Task。创建 Task 后，它会在这里以执行队列形式展示。
      </EmptyState>
    );
  }

  const workspaceName = (workspaceId?: string | null) => {
    if (!workspaceId) return "未绑定 Workspace";
    return workspaces.find((workspace) => workspace.id === workspaceId)?.name ?? workspaceId.slice(0, 8);
  };

  return (
    <div className="overflow-hidden rounded-[8px] border border-border bg-background">
      <div className="hidden grid-cols-[minmax(0,1fr)_7rem_6rem_6rem] gap-3 border-b border-border bg-secondary/20 px-3 py-2 text-[10px] font-medium text-muted-foreground lg:grid">
        <span>Task</span>
        <span>Workspace</span>
        <span>Agent</span>
        <span className="text-right">验收</span>
      </div>
      {tasks.map((task) => (
        <button
          key={task.id}
          type="button"
          onClick={() => onOpenTask(task)}
          className="group grid w-full grid-cols-[auto_minmax(0,1fr)] items-center gap-3 border-b border-border px-3 py-2.5 text-left text-sm transition-colors last:border-b-0 hover:bg-secondary/30 lg:grid-cols-[auto_minmax(0,1fr)_7rem_6rem_6rem]"
        >
          <TaskStatusBadge status={task.status} className="shrink-0 px-2 py-0.5 text-[11px]" />
          <div className="min-w-0 flex-1">
            <div className="flex min-w-0 items-center gap-2">
              <span className="truncate font-medium text-foreground">{task.title}</span>
              {task.lifecycle_step_key && (
                <span className="hidden shrink-0 rounded-[6px] bg-secondary px-1.5 py-0.5 font-mono text-[10px] text-muted-foreground md:inline">
                  {task.lifecycle_step_key}
                </span>
              )}
            </div>
            {task.description && (
              <p className="mt-0.5 truncate text-xs text-muted-foreground">{task.description}</p>
            )}
          </div>
          <span className="hidden min-w-0 truncate text-xs text-muted-foreground lg:block">
            {workspaceName(task.workspace_id)}
          </span>
          <span className="hidden min-w-0 truncate text-xs text-muted-foreground lg:block">
            {task.agent_binding.agent_type ?? task.agent_binding.preset_name ?? "未指定 Agent"}
          </span>
          <span className={`hidden text-right text-xs font-medium lg:block ${taskReviewLabel(task.status).className}`}>
            {taskReviewLabel(task.status).label}
          </span>
        </button>
      ))}
    </div>
  );
}

export function StoryPage() {
  const { storyId } = useParams<{ storyId: string }>();
  const location = useLocation();
  const navigate = useNavigate();
  const projects = useProjectStore((s) => s.projects);
  const storiesByProjectId = useStoryStore((s) => s.storiesByProjectId);
  const tasksByStoryId = useStoryStore((s) => s.tasksByStoryId);
  const sessionsByStoryId = useStoryStore((s) => s.sessionsByStoryId);
  const fetchStoryById = useStoryStore((s) => s.fetchStoryById);
  const fetchTasks = useStoryStore((s) => s.fetchTasks);
  const updateStory = useStoryStore((s) => s.updateStory);
  const deleteStory = useStoryStore((s) => s.deleteStory);
  const error = useStoryStore((s) => s.error);
  const workspacesByProjectId = useWorkspaceStore((s) => s.workspacesByProjectId);

  const [isContextExpanded, setIsContextExpanded] = useState(false);
  const [isDeleteConfirmOpen, setIsDeleteConfirmOpen] = useState(false);
  const [deleteConfirmValue, setDeleteConfirmValue] = useState("");
  const [formMessage, setFormMessage] = useState<string | null>(null);
  const [isEditingBasicInfo, setIsEditingBasicInfo] = useState(false);
  const [isEditingProperties, setIsEditingProperties] = useState(false);
  const [selectedTaskId, setSelectedTaskId] = useState<string | null>(null);
  const routeState = useMemo(
    () => (isStoryNavigationState(location.state) ? location.state : null),
    [location.state],
  );
  const openTaskIdFromRoute = routeState?.open_task_id?.trim() ?? "";

  const story = useMemo(() => (storyId ? findStoryById(storiesByProjectId, storyId) : null), [storiesByProjectId, storyId]);
  const tasks = useMemo(() => (storyId ? tasksByStoryId[storyId] ?? [] : []), [tasksByStoryId, storyId]);
  const sortedTasks = useMemo(
    () => [...tasks].sort((a, b) => new Date(b.updated_at).getTime() - new Date(a.updated_at).getTime()),
    [tasks],
  );
  const routeSelectedTaskId = useMemo(
    () => (openTaskIdFromRoute && sortedTasks.some((task) => task.id === openTaskIdFromRoute) ? openTaskIdFromRoute : null),
    [openTaskIdFromRoute, sortedTasks],
  );
  const effectiveSelectedTaskId = selectedTaskId ?? routeSelectedTaskId;
  const selectedTask = useMemo(
    () => sortedTasks.find((task) => task.id === effectiveSelectedTaskId) ?? null,
    [effectiveSelectedTaskId, sortedTasks],
  );

  const currentProject = useMemo(() => {
    if (!story) return null;
    return projects.find((project) => project.id === story.project_id) ?? null;
  }, [projects, story]);

  const workspaces = useMemo(() => {
    if (!story) return [];
    return workspacesByProjectId[story.project_id] ?? [];
  }, [story, workspacesByProjectId]);

  const sessions = story ? sessionsByStoryId[story.id] ?? [] : [];
  const contextCount = story ? contextSignalCount(story) : 0;

  const [editOverrides, setEditOverrides] = useState<{
    storyId?: string;
    title?: string;
    description?: string;
    status?: StoryStatus;
    priority?: StoryPriority;
    story_type?: StoryType;
    tags?: string;
  }>({});

  const activeEditOverrides = editOverrides.storyId === story?.id ? editOverrides : {};
  const editTitle = activeEditOverrides.title ?? story?.title ?? "";
  const editDescription = activeEditOverrides.description ?? story?.description ?? "";
  const editStatus: StoryStatus = activeEditOverrides.status ?? story?.status ?? "draft";
  const editPriority: StoryPriority = activeEditOverrides.priority ?? story?.priority ?? "p2";
  const editStoryType: StoryType = activeEditOverrides.story_type ?? story?.story_type ?? "feature";
  const editTags = activeEditOverrides.tags ?? story?.tags.join(", ") ?? "";

  const setEditTitle = useCallback(
    (value: string) => setEditOverrides((prev) => ({ ...prev, storyId: story?.id, title: value })),
    [story?.id],
  );
  const setEditDescription = useCallback(
    (value: string) => setEditOverrides((prev) => ({ ...prev, storyId: story?.id, description: value })),
    [story?.id],
  );
  const setEditStatus = useCallback(
    (value: StoryStatus) => setEditOverrides((prev) => ({ ...prev, storyId: story?.id, status: value })),
    [story?.id],
  );
  const setEditPriority = useCallback(
    (value: StoryPriority) => setEditOverrides((prev) => ({ ...prev, storyId: story?.id, priority: value })),
    [story?.id],
  );
  const setEditStoryType = useCallback(
    (value: StoryType) => setEditOverrides((prev) => ({ ...prev, storyId: story?.id, story_type: value })),
    [story?.id],
  );
  const setEditTags = useCallback(
    (value: string) => setEditOverrides((prev) => ({ ...prev, storyId: story?.id, tags: value })),
    [story?.id],
  );

  useEffect(() => {
    if (!storyId || story?.id === storyId) return;
    void fetchStoryById(storyId);
  }, [fetchStoryById, story?.id, storyId]);

  useEffect(() => {
    if (storyId && !tasksByStoryId[storyId]) {
      void fetchTasks(storyId);
    }
  }, [fetchTasks, storyId, tasksByStoryId]);

  const saveStory = useCallback(async (payload: Parameters<typeof updateStory>[1]) => {
    if (!story) return false;
    const updated = await updateStory(story.id, payload);
    return Boolean(updated);
  }, [story, updateStory]);

  const handleSaveBasicInfo = async () => {
    const trimmedTitle = editTitle.trim();
    if (!trimmedTitle) {
      setFormMessage("Story 标题不能为空");
      return;
    }

    const ok = await saveStory({
      title: trimmedTitle,
      description: editDescription,
    });
    if (!ok) return;

    setFormMessage(null);
    setIsEditingBasicInfo(false);
    setEditOverrides((prev) => ({ ...prev, title: undefined, description: undefined }));
  };

  const handleSaveProperties = async () => {
    const parsedTags = editTags
      .split(",")
      .map((item) => item.trim())
      .filter((item) => item.length > 0);

    const ok = await saveStory({
      status: editStatus,
      priority: editPriority,
      story_type: editStoryType,
      tags: parsedTags,
    });
    if (!ok) return;

    setFormMessage(null);
    setIsEditingProperties(false);
    setEditOverrides((prev) => ({
      ...prev,
      status: undefined,
      priority: undefined,
      story_type: undefined,
      tags: undefined,
    }));
  };

  const handleDeleteStory = async () => {
    if (!story) return;
    if (deleteConfirmValue.trim() !== story.title) {
      setFormMessage("请输入完整 Story 标题后再删除");
      return;
    }
    await deleteStory(story.id);
    setIsDeleteConfirmOpen(false);
    navigate("/dashboard/story");
  };

  const handleTaskCreated = () => {
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

  const handleTaskDeleted = (taskId: string) => {
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
          <Button type="button" variant="primary" className="mt-4" onClick={() => navigate("/dashboard/story")}>
            返回 Story
          </Button>
        </div>
      </div>
    );
  }

  return (
    <div className="flex h-full flex-col overflow-hidden">
      <header className="flex h-14 shrink-0 items-center justify-between border-b border-border bg-background px-6">
        <div className="flex min-w-0 items-center gap-3">
          <button
            type="button"
            onClick={() => navigate("/dashboard/story")}
            className="inline-flex h-8 shrink-0 items-center gap-1.5 rounded-[8px] px-2 text-xs font-medium text-muted-foreground transition-colors hover:bg-secondary/50 hover:text-foreground"
          >
            <BackToStoriesIcon />
            Stories
          </button>
          <div className="flex min-w-0 items-center gap-2.5">
            <span className="agentdash-panel-header-tag">Story</span>
            <div className="min-w-0">
              <h1 className="truncate text-sm font-semibold text-foreground">{story.title}</h1>
              <p className="truncate text-xs text-muted-foreground">{currentProject?.name ?? story.project_id}</p>
            </div>
          </div>
        </div>
        <div className="flex items-center gap-2">
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

      <div className="flex flex-1 overflow-hidden bg-background">
        <main className="min-w-0 flex-1 overflow-y-auto">
          <div className="mx-auto max-w-7xl space-y-4 px-4 py-4">
            <section className="grid h-[calc(100vh-7rem)] gap-4 lg:grid-cols-[minmax(0,1fr)_18rem] xl:grid-cols-[minmax(0,1fr)_22rem]">
              <div className="flex min-h-0 flex-col overflow-hidden rounded-[8px] border border-border bg-background">
                <SectionTitle
                  title="关联 Session"
                  badge={`${sessions.length}`}
                />
                <div className="min-h-0 flex-1 overflow-hidden border-t border-border">
                  <StorySessionPanel story={story} />
                </div>
              </div>

              <div className="flex min-h-0 flex-col overflow-hidden rounded-[8px] border border-border bg-background">
                <SectionTitle title="Story Details" />
                <div className="min-h-0 flex-1 space-y-4 overflow-y-auto border-t border-border p-4">
                  <section className="border-b border-border pb-4">
                    <div className="mb-3 flex items-start justify-between gap-3">
                      <p className="text-xs font-medium text-muted-foreground">Brief</p>
                      {isEditingBasicInfo ? (
                        <div className="flex shrink-0 gap-2">
                          <Button type="button" variant="secondary" size="sm" onClick={() => { setIsEditingBasicInfo(false); setEditOverrides({}); }}>
                            取消
                          </Button>
                          <Button type="button" variant="primary" size="sm" onClick={() => void handleSaveBasicInfo()}>
                            保存
                          </Button>
                        </div>
                      ) : (
                        <Button type="button" variant="secondary" size="sm" onClick={() => setIsEditingBasicInfo(true)}>
                          编辑
                        </Button>
                      )}
                    </div>

                    {isEditingBasicInfo ? (
                      <div className="space-y-3">
                        <TextInput
                          value={editTitle}
                          onChange={(event) => setEditTitle(event.target.value)}
                          placeholder="Story 标题"
                          className="text-sm font-semibold"
                          autoFocus
                        />
                        <Textarea
                          value={editDescription}
                          onChange={(event) => setEditDescription(event.target.value)}
                          rows={6}
                          placeholder="Story 描述 / 验收口径 / 实现边界"
                        />
                      </div>
                    ) : (
                      <div className="space-y-2">
                        <h2 className="text-sm font-semibold text-foreground">{story.title}</h2>
                        <p className="whitespace-pre-wrap text-xs leading-5 text-muted-foreground">
                          {story.description || "暂无 Story brief。"}
                        </p>
                      </div>
                    )}
                    {(formMessage || error) && <p className="mt-3 text-xs text-destructive">{formMessage || error}</p>}
                  </section>

                  <section className="border-b border-border pb-4">
                    <div className="mb-3 flex items-center justify-between gap-3">
                      <p className="text-xs font-medium text-muted-foreground">Properties</p>
                      {!isEditingProperties ? (
                        <Button type="button" variant="secondary" size="sm" onClick={() => setIsEditingProperties(true)}>
                          编辑
                        </Button>
                      ) : null}
                    </div>

                    {isEditingProperties ? (
                      <div className="grid grid-cols-2 gap-3">
                        <Field label="状态">
                          <Select
                            value={editStatus}
                            onChange={(event) => setEditStatus(event.target.value as StoryStatus)}
                          >
                            <option value="draft">draft</option>
                            <option value="ready">ready</option>
                            <option value="running">running</option>
                            <option value="review">review</option>
                            <option value="completed">completed</option>
                            <option value="failed">failed</option>
                            <option value="cancelled">cancelled</option>
                          </Select>
                        </Field>
                        <Field label="优先级">
                          <Select
                            value={editPriority}
                            onChange={(event) => setEditPriority(event.target.value as StoryPriority)}
                          >
                            {priorityOptions.map((opt) => (
                              <option key={opt.value} value={opt.value}>
                                {opt.label}
                              </option>
                            ))}
                          </Select>
                        </Field>
                        <Field label="类型">
                          <Select
                            value={editStoryType}
                            onChange={(event) => setEditStoryType(event.target.value as StoryType)}
                          >
                            {storyTypeOptions.map((opt) => (
                              <option key={opt.value} value={opt.value}>
                                {opt.label}
                              </option>
                            ))}
                          </Select>
                        </Field>
                        <Field label="标签" className="col-span-2">
                          <TextInput
                            value={editTags}
                            onChange={(event) => setEditTags(event.target.value)}
                            placeholder="frontend, api"
                          />
                        </Field>
                        <div className="col-span-2 flex gap-2">
                          <Button type="button" variant="secondary" size="sm" className="flex-1" onClick={() => { setIsEditingProperties(false); setEditOverrides({}); }}>
                            取消
                          </Button>
                          <Button type="button" variant="primary" size="sm" className="flex-1" onClick={() => void handleSaveProperties()}>
                            保存
                          </Button>
                        </div>
                      </div>
                    ) : (
                      <div className="space-y-3">
                        <div className="grid grid-cols-2 gap-x-3 gap-y-3">
                          <CompactProperty label="状态">
                            <StoryStatusBadge status={story.status} className="px-2 py-0.5 text-[11px]" />
                          </CompactProperty>
                          <CompactProperty label="优先级">
                            <StoryPriorityBadge priority={story.priority} showLabel />
                          </CompactProperty>
                          <CompactProperty label="类型">
                            <StoryTypeBadge type={story.story_type} />
                          </CompactProperty>
                          <CompactProperty label="Context">
                            <span className="font-medium text-foreground">{contextCount}</span>
                          </CompactProperty>
                          <CompactProperty label="标签" className="col-span-2">
                            {story.tags.length > 0 ? (
                              <div className="flex flex-wrap gap-1.5">
                                {story.tags.map((tag) => (
                                  <span key={tag} className="rounded-[6px] bg-secondary px-1.5 py-0.5 text-xs text-muted-foreground">
                                    {tag}
                                  </span>
                                ))}
                              </div>
                            ) : (
                              <span className="text-muted-foreground">暂无标签</span>
                            )}
                          </CompactProperty>
                        </div>
                        <div className="pt-1">
                          <StoryStatusActions currentStatus={story.status} onStatusChange={(status) => void updateStory(story.id, { status })} />
                        </div>
                      </div>
                    )}
                  </section>

                  <section className="border-b border-border pb-4">
                    <div className="mb-3 flex items-center justify-between gap-3">
                      <p className="text-xs font-medium text-muted-foreground">Context</p>
                      <Button type="button" variant="ghost" size="sm" onClick={() => setIsContextExpanded((value) => !value)}>
                        {isContextExpanded ? "收起" : "编辑"}
                      </Button>
                    </div>
                    <div className="flex flex-wrap gap-2">
                      {Array.from(contextSummary(story.context.source_refs)).map(([kind, count]) => {
                        const meta = sourceKindMeta(kind);
                        return (
                          <span key={kind} className={`rounded-[6px] bg-secondary px-2 py-1 text-[11px] font-medium ${meta.color}`}>
                            {meta.icon} · {count} {meta.label}
                          </span>
                        );
                      })}
                      {story.context.context_containers.length > 0 && (
                        <Badge variant="accent">{story.context.context_containers.length} VFS Mount</Badge>
                      )}
                      {story.context.disabled_container_ids.length > 0 && (
                        <Badge variant="warning">{story.context.disabled_container_ids.length} 禁用继承</Badge>
                      )}
                      {story.context.session_composition && <Badge variant="info">会话编排</Badge>}
                      {contextCount === 0 && <span className="text-xs text-muted-foreground">暂无显式 Story 上下文。</span>}
                    </div>
                  </section>

                  <section className="border-t border-border pt-4">
                    <SectionTitle
                      title="Tasks"
                      subtitle="验收状态直接随 Task 行展示"
                      badge={`${sortedTasks.length}`}
                    />
                    <div className="space-y-3 pt-3">
                      <CreateTaskPanel
                        story={story}
                        storyId={story.id}
                        workspaces={workspaces}
                        projectConfig={currentProject?.config}
                        onCreated={handleTaskCreated}
                      />
                      <StoryTaskRows tasks={sortedTasks} workspaces={workspaces} onOpenTask={(task) => setSelectedTaskId(task.id)} />
                    </div>
                  </section>

                  <section className="border-t border-border pt-4">
                    <SectionTitle title="Metadata" />
                    <div className="space-y-3 pt-3 text-sm">
                      <InspectorRow label="Story ID" value={story.id} mono />
                      <InspectorRow label="Project ID" value={story.project_id} mono />
                      <InspectorRow label="默认 Workspace" value={story.default_workspace_id ?? "继承 Project 默认"} mono={Boolean(story.default_workspace_id)} />
                      <InspectorRow label="创建" value={formatDate(story.created_at)} />
                      <InspectorRow label="更新" value={formatDate(story.updated_at)} />
                    </div>
                  </section>
                </div>
              </div>
            </section>

            {isContextExpanded && (
              <section className="overflow-hidden rounded-[8px] border border-border bg-background">
                <SectionTitle title="Context Advanced" subtitle="管理 Story 的上下文源、VFS Mount、继承禁用与会话编排。" badge={`${contextCount}`} />
                <div className="border-t border-border p-4">
                  <ContextPanel
                    story={story}
                    workspaces={workspaces}
                    projectConfig={currentProject?.config}
                  />
                </div>
              </section>
            )}

          </div>
        </main>

      </div>

      <TaskDrawer
        key={selectedTask?.id ?? "no-task-selected"}
        task={selectedTask}
        workspaces={workspaces}
        projectConfig={currentProject?.config}
        onTaskUpdated={handleTaskUpdated}
        onTaskDeleted={handleTaskDeleted}
        onClose={() => {
          setSelectedTaskId(null);
          if (routeSelectedTaskId) {
            navigate(location.pathname, { replace: true, state: null });
          }
        }}
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
    </div>
  );
}
