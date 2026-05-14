import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useLocation, useNavigate, useParams } from "react-router-dom";
import type {
  ContextSourceKind,
  StoryNavigationState,
  StoryStatus,
  StoryPriority,
  StoryType,
  Task,
} from "../types";
import { StorySessionPanel } from "../features/story/story-session-panel";
import { StoryStatusBadge, StoryPriorityBadge, StoryTypeBadge } from "../components/ui/status-badge";
import { TaskList } from "../features/task/task-list";
import { TaskDrawer } from "../features/task/task-drawer";
import { useStoryStore, findStoryById } from "../stores/storyStore";
import { useProjectStore } from "../stores/projectStore";
import { useWorkspaceStore } from "../stores/workspaceStore";
import {
  DangerConfirmDialog,
  DetailMenu,
  DetailSection,
} from "../components/ui/detail-panel";
import { CreateTaskPanel } from "../features/story/create-task-panel";
import { sourceKindMeta } from "../features/story/context-source-utils";
import { ContextPanel, ReviewPanel } from "../features/story/story-detail-panels";

const priorityOptions: { value: StoryPriority; label: string }[] = [
  { value: "p0", label: "P0 - 紧急" },
  { value: "p1", label: "P1 - 高" },
  { value: "p2", label: "P2 - 中" },
  { value: "p3", label: "P3 - 低" },
];

const storyTypeOptions: { value: StoryType; label: string; icon: string }[] = [
  { value: "feature", label: "功能", icon: "✨" },
  { value: "bugfix", label: "缺陷", icon: "🐛" },
  { value: "refactor", label: "重构", icon: "♻️" },
  { value: "docs", label: "文档", icon: "📝" },
  { value: "test", label: "测试", icon: "🧪" },
  { value: "other", label: "其他", icon: "📦" },
];

type TabKey = "tasks" | "sessions" | "review";

function contextSummary(sourceRefs: { kind: ContextSourceKind }[]) {
  const counts = new Map<ContextSourceKind, number>();
  for (const ref of sourceRefs) {
    counts.set(ref.kind, (counts.get(ref.kind) ?? 0) + 1);
  }
  return counts;
}

// ─── StoryStatusActions ────────────────────────────────

interface StoryStatusActionsProps {
  currentStatus: StoryStatus;
  onStatusChange: (status: StoryStatus) => void;
}

function StoryStatusActions({ currentStatus, onStatusChange }: StoryStatusActionsProps) {
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
              className={`rounded-[10px] px-3 py-1.5 text-xs font-medium transition-colors ${variantClasses[action.variant]}`}
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

// ─── StoryPage ─────────────────────────────────────────

export function StoryPage() {
  const { storyId } = useParams<{ storyId: string }>();
  const location = useLocation();
  const navigate = useNavigate();
  const { projects } = useProjectStore();
  const {
    storiesByProjectId,
    tasksByStoryId,
    fetchStoryById,
    fetchTasks,
    updateStory,
    deleteStory,
    error,
  } = useStoryStore();
  const { workspacesByProjectId } = useWorkspaceStore();

  const [activeTab, setActiveTab] = useState<TabKey>("sessions");
  const [isContextExpanded, setIsContextExpanded] = useState(false);
  const [isDeleteConfirmOpen, setIsDeleteConfirmOpen] = useState(false);
  const [deleteConfirmValue, setDeleteConfirmValue] = useState("");
  const [formMessage, setFormMessage] = useState<string | null>(null);
  const [isEditingBasicInfo, setIsEditingBasicInfo] = useState(false);
  const [selectedTaskId, setSelectedTaskId] = useState<string | null>(null);
  const fetchingStoryIdRef = useRef<string | null>(null);
  const routeState = useMemo(
    () => (location.state as StoryNavigationState | null) ?? null,
    [location.state],
  );
  const openTaskIdFromRoute = routeState?.open_task_id?.trim() ?? "";

  const story = useMemo(() => (storyId ? findStoryById(storiesByProjectId, storyId) : null), [storiesByProjectId, storyId]);

  const [editOverrides, setEditOverrides] = useState<{
    title?: string; description?: string; status?: StoryStatus;
    priority?: StoryPriority; story_type?: StoryType; tags?: string;
  }>({});

  const editTitle = editOverrides.title ?? story?.title ?? "";
  const editDescription = editOverrides.description ?? story?.description ?? "";
  const editStatus: StoryStatus = editOverrides.status ?? story?.status ?? "draft";
  const editPriority: StoryPriority = editOverrides.priority ?? story?.priority ?? "p2";
  const editStoryType: StoryType = editOverrides.story_type ?? story?.story_type ?? "feature";
  const editTags = editOverrides.tags ?? story?.tags.join(", ") ?? "";

  const setEditTitle = useCallback((v: string) => setEditOverrides(prev => ({ ...prev, title: v })), []);
  const setEditDescription = useCallback((v: string) => setEditOverrides(prev => ({ ...prev, description: v })), []);
  const setEditStatus = useCallback((v: StoryStatus) => setEditOverrides(prev => ({ ...prev, status: v })), []);
  const setEditPriority = useCallback((v: StoryPriority) => setEditOverrides(prev => ({ ...prev, priority: v })), []);
  const setEditStoryType = useCallback((v: StoryType) => setEditOverrides(prev => ({ ...prev, story_type: v })), []);
  const setEditTags = useCallback((v: string) => setEditOverrides(prev => ({ ...prev, tags: v })), []);

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

  useEffect(() => {
    if (!storyId || story?.id === storyId) return;

    fetchingStoryIdRef.current = storyId;
    void fetchStoryById(storyId);
  }, [fetchStoryById, story?.id, storyId]);

  const isStoryLoading = !!storyId && story?.id !== storyId;

  useEffect(() => {
    if (storyId && !tasksByStoryId[storyId]) {
      void fetchTasks(storyId);
    }
  }, [storyId, tasksByStoryId, fetchTasks]);

  useEffect(() => {
    if (!openTaskIdFromRoute) return;
    if (selectedTaskId === openTaskIdFromRoute) return;

    const matched = sortedTasks.some((task) => task.id === openTaskIdFromRoute);
    if (!matched) return;

    // eslint-disable-next-line react-hooks/set-state-in-effect -- 与 router 外部状态同步
    setSelectedTaskId(openTaskIdFromRoute);
    navigate(location.pathname, { replace: true, state: null });
  }, [location.pathname, navigate, openTaskIdFromRoute, selectedTaskId, sortedTasks]);


  const handleSaveStory = async () => {
    if (!story) return;
    const trimmedTitle = editTitle.trim();
    if (!trimmedTitle) {
      setFormMessage("Story 标题不能为空");
      return;
    }

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

  if (!story && isStoryLoading) {
    return (
      <div className="flex h-full items-center justify-center">
        <div className="text-center">
          <div className="mx-auto h-6 w-6 animate-spin rounded-full border-2 border-primary border-t-transparent" />
          <p className="mt-3 text-sm text-muted-foreground">正在加载 Story...</p>
        </div>
      </div>
    );
  }

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
    { key: "sessions", label: "会话" },
    { key: "tasks", label: "任务列表" },
    { key: "review", label: "验收" },
  ] as const;

  return (
    <div className="flex h-full flex-col overflow-hidden">
      <header className="flex h-14 shrink-0 items-center justify-between border-b border-border bg-background px-6">
        <div className="flex items-center gap-3">
          <button
            type="button"
            onClick={() => navigate("/")}
            className="rounded-[10px] border border-border bg-background px-2.5 py-1.5 text-sm text-muted-foreground transition-colors hover:bg-secondary hover:text-foreground"
          >
            ← 返回看板
          </button>
          <div className="flex items-center gap-2.5">
            <span className="agentdash-panel-header-tag">Story</span>
            <div>
            <h1 className="text-sm font-semibold text-foreground">{story.title}</h1>
            <p className="text-xs text-muted-foreground">ID: {story.id}</p>
            </div>
          </div>
        </div>
        <div className="flex items-center gap-2">
          <StoryTypeBadge type={story.story_type} />
          <StoryPriorityBadge priority={story.priority} showLabel />
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

      <div className="flex flex-1 overflow-hidden">
        {/* 左侧：Story 编辑 */}
        <div className="w-80 shrink-0 overflow-y-auto border-r border-border bg-background p-4">
          <DetailSection
            title="基本信息"
            extra={
              !isEditingBasicInfo && (
                <button
                  type="button"
                  onClick={() => setIsEditingBasicInfo(true)}
                  className="rounded-[8px] border border-border bg-background px-2 py-1 text-xs text-muted-foreground transition-colors hover:bg-secondary hover:text-foreground"
                >
                  编辑
                </button>
              )
            }
          >
            {isEditingBasicInfo ? (
              <div className="space-y-3">
                <div>
                  <label className="agentdash-form-label">标题</label>
                  <input
                    value={editTitle}
                    onChange={(event) => setEditTitle(event.target.value)}
                    placeholder="Story 标题"
                    autoFocus
                    className="agentdash-form-input"
                  />
                </div>
                <div>
                  <label className="agentdash-form-label">描述</label>
                  <textarea
                    value={editDescription}
                    onChange={(event) => setEditDescription(event.target.value)}
                    rows={3}
                    placeholder="Story 描述"
                    className="agentdash-form-textarea"
                  />
                </div>
                <div>
                  <label className="agentdash-form-label">类型</label>
                  <select
                    value={editStoryType}
                    onChange={(event) => setEditStoryType(event.target.value as StoryType)}
                    className="agentdash-form-select"
                  >
                    {storyTypeOptions.map((opt) => (
                      <option key={opt.value} value={opt.value}>
                        {opt.icon} {opt.label}
                      </option>
                    ))}
                  </select>
                </div>
                <div>
                  <label className="agentdash-form-label">优先级</label>
                  <select
                    value={editPriority}
                    onChange={(event) => setEditPriority(event.target.value as StoryPriority)}
                    className="agentdash-form-select"
                  >
                    {priorityOptions.map((opt) => (
                      <option key={opt.value} value={opt.value}>
                        {opt.label}
                      </option>
                    ))}
                  </select>
                </div>
                <div>
                  <label className="agentdash-form-label">标签（逗号分隔）</label>
                  <input
                    value={editTags}
                    onChange={(event) => setEditTags(event.target.value)}
                    placeholder="例如: frontend, api, urgent"
                    className="agentdash-form-input"
                  />
                </div>
                <div className="flex gap-2">
                  <button
                    type="button"
                    onClick={() => {
                      setIsEditingBasicInfo(false);
                      if (story) {
                        setEditTitle(story.title);
                        setEditDescription(story.description || "");
                        setEditStatus(story.status);
                        setEditPriority(story.priority);
                        setEditStoryType(story.story_type);
                        setEditTags(story.tags.join(", "));
                      }
                    }}
                    className="agentdash-button-secondary flex-1"
                  >
                    取消
                  </button>
                  <button
                    type="button"
                    onClick={() => void handleSaveStory()}
                    className="agentdash-button-primary flex-1"
                  >
                    保存
                  </button>
                </div>
              </div>
            ) : (
              <div className="space-y-3.5">
                <div>
                  <span className="text-xs text-muted-foreground">标题</span>
                  <p className="mt-1 text-sm font-medium">{story.title}</p>
                </div>
                <div>
                  <span className="text-xs text-muted-foreground">描述</span>
                  <p className="mt-1 text-sm leading-6 text-foreground">
                    {story.description || <span className="text-muted-foreground">暂无描述</span>}
                  </p>
                </div>
                <div>
                  <span className="text-xs text-muted-foreground">类型</span>
                  <div className="mt-1.5">
                    <StoryTypeBadge type={story.story_type} />
                  </div>
                </div>
                <div>
                  <span className="text-xs text-muted-foreground">优先级</span>
                  <div className="mt-1.5">
                    <StoryPriorityBadge priority={story.priority} showLabel />
                  </div>
                </div>
                {story.tags.length > 0 && (
                  <div>
                    <span className="text-xs text-muted-foreground">标签</span>
                    <div className="mt-1.5 flex flex-wrap gap-1.5">
                      {story.tags.map((tag, index) => (
                        <span
                          key={index}
                          className="inline-flex items-center rounded-full border border-border bg-background px-2.5 py-1 text-xs text-muted-foreground"
                        >
                          {tag}
                        </span>
                      ))}
                    </div>
                  </div>
                )}
              </div>
            )}
          </DetailSection>

          <div className="mt-3">
            <StoryStatusActions
              currentStatus={story.status}
              onStatusChange={(status) => void updateStory(story.id, { status })}
            />
          </div>

          <div className="mt-3">
            <CreateTaskPanel
              story={story}
              storyId={story.id}
              workspaces={workspaces}
              projectConfig={currentProject?.config}
              onCreated={handleTaskCreated}
            />
          </div>

          {(formMessage || error) && <p className="mt-3 text-xs text-destructive">{formMessage || error}</p>}
        </div>

        {/* 右侧：Tab 内容 */}
        <div className="flex flex-1 flex-col overflow-hidden bg-background">
          <div className="shrink-0 border-b border-border">
            <button
              type="button"
              onClick={() => setIsContextExpanded((v) => !v)}
              className="flex w-full items-center justify-between px-5 py-2.5 text-xs text-muted-foreground transition-colors hover:bg-secondary/25"
            >
              <div className="flex items-center gap-2">
                <svg
                  className={`h-3.5 w-3.5 transition-transform ${isContextExpanded ? "rotate-90" : ""}`}
                  fill="none"
                  viewBox="0 0 24 24"
                  stroke="currentColor"
                >
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M9 5l7 7-7 7" />
                </svg>
                <span className="font-medium">上下文</span>
                {Array.from(contextSummary(story.context.source_refs)).map(([kind, count]) => {
                  const meta = sourceKindMeta(kind);
                  return (
                    <span key={kind} className={`rounded-full border border-current/20 px-1.5 py-0.5 text-[10px] font-medium ${meta.color}`}>
                      {meta.icon} {count} {meta.label}
                    </span>
                  );
                })}
                {story.context.context_containers.length > 0 && (
                  <span className="rounded-full border border-violet-400/30 bg-violet-500/10 px-1.5 py-0.5 text-[10px] font-medium text-violet-600">
                    {story.context.context_containers.length} 容器
                  </span>
                )}
                {story.context.session_composition && (
                  <span className="rounded-full border border-cyan-400/30 bg-cyan-500/10 px-1.5 py-0.5 text-[10px] font-medium text-cyan-600">
                    会话编排
                  </span>
                )}
              </div>
            </button>
            {isContextExpanded && (
              <div className="max-h-[40vh] overflow-y-auto border-t border-border bg-secondary/10 px-5 py-4">
                <ContextPanel
                  story={story}
                  workspaces={workspaces}
                  projectConfig={currentProject?.config}
                />
              </div>
            )}
          </div>

          <div className="flex border-b border-border bg-secondary/35 px-2 pt-2">
            {tabs.map((tab) => (
              <button
                key={tab.key}
                type="button"
                onClick={() => setActiveTab(tab.key)}
                className={`rounded-t-[10px] px-5 py-3 text-sm transition-colors ${
                  activeTab === tab.key
                    ? "border border-border border-b-background bg-background font-medium text-foreground"
                    : "text-muted-foreground hover:text-foreground"
                }`}
              >
                {tab.label}
              </button>
            ))}
          </div>

          {activeTab === "sessions" ? (
            <div className="flex-1 overflow-hidden">
              <StorySessionPanel story={story} />
            </div>
          ) : (
            <div className="flex-1 overflow-y-auto p-6">
              {activeTab === "tasks" && (
                <DetailSection title="任务列表">
                  <TaskList
                    tasks={sortedTasks}
                    onTaskClick={(task) => {
                      setSelectedTaskId(task.id);
                    }}
                  />
                </DetailSection>
              )}
              {activeTab === "review" && <ReviewPanel story={story} tasks={sortedTasks} />}
            </div>
          )}
        </div>
      </div>

      <TaskDrawer
        key={selectedTask?.id ?? "no-task-selected"}
        task={selectedTask}
        workspaces={workspaces}
        projectConfig={currentProject?.config}
        onTaskUpdated={handleTaskUpdated}
        onTaskDeleted={handleTaskDeleted}
        onClose={() => setSelectedTaskId(null)}
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
