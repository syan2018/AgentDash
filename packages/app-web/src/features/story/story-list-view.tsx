import { useEffect, useMemo, useRef, useState } from "react";
import type { Story, StoryPriority, StoryStatus, StoryType } from "../../types";
import { StoryBoard } from "./story-board";
import { useStoryStore } from "../../stores/storyStore";
import {
  Button,
  CreateButton,
  DetailPanel,
  EmptyState,
  SectionTitle,
  Textarea,
  TextInput,
} from "@agentdash/ui";
import {
  StoryPriorityBadge,
  StoryStatusBadge,
  StoryTypeBadge,
} from "../../components/ui/status-badge";
import {
  PropertyPicker,
  type PropertyPickerOption,
} from "../../components/ui/property-picker";
import { useStoryViewStore } from "../../stores/storyViewStore";
import { selectFilteredStories, activeFilterCount } from "./select-stories";
import { StoryToolbar } from "./story-toolbar";
import {
  EditablePriorityBadge,
  EditableStatusBadge,
  EditableTypeBadge,
} from "./story-edit-badges";

const CREATE_STATUS_OPTIONS: PropertyPickerOption<StoryStatus>[] = [
  { value: "draft", label: "draft", preview: <StoryStatusBadge status="draft" /> },
  { value: "ready", label: "ready", preview: <StoryStatusBadge status="ready" /> },
  { value: "running", label: "running", preview: <StoryStatusBadge status="running" /> },
  { value: "review", label: "review", preview: <StoryStatusBadge status="review" /> },
  { value: "completed", label: "completed", preview: <StoryStatusBadge status="completed" /> },
  { value: "failed", label: "failed", preview: <StoryStatusBadge status="failed" /> },
  { value: "cancelled", label: "cancelled", preview: <StoryStatusBadge status="cancelled" /> },
];

const CREATE_PRIORITY_OPTIONS: PropertyPickerOption<StoryPriority>[] = [
  { value: "p0", label: "P0 紧急", preview: <StoryPriorityBadge priority="p0" /> },
  { value: "p1", label: "P1 高", preview: <StoryPriorityBadge priority="p1" /> },
  { value: "p2", label: "P2 中", preview: <StoryPriorityBadge priority="p2" /> },
  { value: "p3", label: "P3 低", preview: <StoryPriorityBadge priority="p3" /> },
];

const CREATE_TYPE_OPTIONS: PropertyPickerOption<StoryType>[] = [
  { value: "feature", label: "feature 功能", preview: <StoryTypeBadge type="feature" /> },
  { value: "bugfix", label: "bugfix 缺陷", preview: <StoryTypeBadge type="bugfix" /> },
  { value: "refactor", label: "refactor 重构", preview: <StoryTypeBadge type="refactor" /> },
  { value: "docs", label: "docs 文档", preview: <StoryTypeBadge type="docs" /> },
  { value: "test", label: "test 测试", preview: <StoryTypeBadge type="test" /> },
  { value: "other", label: "other 其他", preview: <StoryTypeBadge type="other" /> },
];

interface StoryListViewProps {
  stories: Story[];
  taskCountByStoryId: Record<string, number>;
  onOpenStory: (story: Story) => void;
  projectId: string;
}

function parseTags(input: string): string[] {
  return input
    .split(",")
    .map((item) => item.trim())
    .filter((item) => item.length > 0);
}

interface CreateStoryDrawerProps {
  initialStatus: StoryStatus;
  open: boolean;
  projectId: string;
  onClose: () => void;
}

function CreateStoryDrawer({
  initialStatus,
  open,
  projectId,
  onClose,
}: CreateStoryDrawerProps) {
  const { createStory, error } = useStoryStore();
  const [title, setTitle] = useState("");
  const [description, setDescription] = useState("");
  const [status, setStatus] = useState<StoryStatus>(initialStatus);
  const [priority, setPriority] = useState<StoryPriority>("p2");
  const [storyType, setStoryType] = useState<StoryType>("feature");
  const [tags, setTags] = useState("");
  const [formMessage, setFormMessage] = useState<string | null>(null);
  const titleInputRef = useRef<HTMLInputElement>(null);
  const [openSnapshot, setOpenSnapshot] = useState(open);

  if (open !== openSnapshot) {
    setOpenSnapshot(open);
    if (open) {
      setStatus(initialStatus);
      setFormMessage(null);
    }
  }

  useEffect(() => {
    if (open) {
      const t = window.setTimeout(() => titleInputRef.current?.focus(), 100);
      return () => window.clearTimeout(t);
    }
  }, [open]);

  const resetForm = () => {
    setTitle("");
    setDescription("");
    setPriority("p2");
    setStoryType("feature");
    setTags("");
    setFormMessage(null);
  };

  const handleCreate = async () => {
    const trimmedTitle = title.trim();
    if (!trimmedTitle) {
      setFormMessage("Story 标题不能为空");
      return;
    }

    const created = await createStory(projectId, trimmedTitle, description.trim() || undefined, {
      status,
      priority,
      story_type: storyType,
      tags: parseTags(tags),
    });
    if (!created) return;

    resetForm();
    onClose();
  };

  return (
    <DetailPanel
      open={open}
      title="新建 Story"
      subtitle="创建后可在详情中继续编辑、补充上下文并拆分 Task"
      onClose={onClose}
      widthClassName="max-w-2xl"
    >
      <div className="flex h-full flex-col">
        <div className="flex-1 space-y-5 overflow-y-auto px-5 py-5">
          <div>
            <input
              ref={titleInputRef}
              value={title}
              onChange={(e) => setTitle(e.target.value)}
              placeholder="Story 标题 — 用户可感知的一段交付目标"
              className="w-full bg-transparent text-base font-semibold text-foreground outline-none placeholder:text-muted-foreground/60"
            />
            <Textarea
              value={description}
              onChange={(e) => setDescription(e.target.value)}
              rows={6}
              placeholder="补充背景、验收口径或实现边界（可选）"
              className="mt-2 resize-none border-0 bg-transparent p-0 text-sm leading-6 shadow-none focus:ring-0"
            />
          </div>

          <div className="flex flex-wrap items-center gap-x-5 gap-y-2 border-t border-border pt-4">
            <div className="flex items-center gap-2">
              <span className="text-[11px] font-medium text-muted-foreground">状态</span>
              <PropertyPicker<StoryStatus>
                triggerLabel={`status: ${status}`}
                trigger={<StoryStatusBadge status={status} />}
                value={status}
                options={CREATE_STATUS_OPTIONS}
                onChange={setStatus}
              />
            </div>
            <div className="flex items-center gap-2">
              <span className="text-[11px] font-medium text-muted-foreground">优先级</span>
              <PropertyPicker<StoryPriority>
                triggerLabel={`priority: ${priority}`}
                trigger={<StoryPriorityBadge priority={priority} />}
                value={priority}
                options={CREATE_PRIORITY_OPTIONS}
                onChange={setPriority}
              />
            </div>
            <div className="flex items-center gap-2">
              <span className="text-[11px] font-medium text-muted-foreground">类型</span>
              <PropertyPicker<StoryType>
                triggerLabel={`type: ${storyType}`}
                trigger={<StoryTypeBadge type={storyType} />}
                value={storyType}
                options={CREATE_TYPE_OPTIONS}
                onChange={setStoryType}
              />
            </div>
          </div>

          <div className="border-t border-border pt-4">
            <p className="mb-1.5 text-[11px] font-medium text-muted-foreground">标签</p>
            <TextInput
              value={tags}
              onChange={(e) => setTags(e.target.value)}
              placeholder="用逗号分隔，例如: frontend, api, urgent"
            />
            {parseTags(tags).length > 0 && (
              <div className="mt-2 flex flex-wrap gap-1">
                {parseTags(tags).map((tag) => (
                  <span
                    key={tag}
                    className="inline-flex items-center rounded-[6px] bg-secondary px-1.5 py-0.5 text-[11px] text-muted-foreground"
                  >
                    {tag}
                  </span>
                ))}
              </div>
            )}
          </div>

          {(formMessage || error) && (
            <p className="text-xs text-destructive">{formMessage || error}</p>
          )}
        </div>

        <footer className="flex shrink-0 items-center justify-end gap-3 border-t border-border bg-background px-5 py-3">
          <Button
            type="button"
            variant="primary"
            size="sm"
            onClick={() => void handleCreate()}
            disabled={!title.trim()}
          >
            创建 Story
          </Button>
        </footer>
      </div>
    </DetailPanel>
  );
}

interface StoryListRowProps {
  story: Story;
  taskCount: number;
  onOpenStory: (story: Story) => void;
}

function StoryListRow({ story, taskCount, onOpenStory }: StoryListRowProps) {
  const isSelected = useStoryViewStore((s) => s.selectedIds.has(story.id));
  const toggleSelect = useStoryViewStore((s) => s.toggleSelect);
  const setFocused = useStoryViewStore((s) => s.setFocusedStory);

  const contextCount =
    story.context.source_refs.length +
    story.context.context_containers.length +
    story.context.disabled_container_ids.length +
    (story.context.session_composition ? 1 : 0);

  const handleRowClick = (event: React.MouseEvent) => {
    if (event.metaKey || event.ctrlKey) {
      event.preventDefault();
      toggleSelect(story.id);
      return;
    }
    onOpenStory(story);
  };

  const handleKeyDown = (event: React.KeyboardEvent) => {
    if (event.key === "Enter") {
      event.preventDefault();
      onOpenStory(story);
    }
  };

  return (
    <div
      role="button"
      tabIndex={0}
      data-story-card-id={story.id}
      onClick={handleRowClick}
      onKeyDown={handleKeyDown}
      onFocus={() => setFocused(story.id)}
      onBlur={() => setFocused(null)}
      className={`group/row grid min-h-12 w-full cursor-pointer grid-cols-[minmax(0,1fr)] items-center gap-3 border-b border-border px-4 py-2 text-left text-sm outline-none transition-colors focus-visible:bg-secondary/40 lg:grid-cols-[2rem_8rem_minmax(0,1fr)_5rem_5rem_5rem_5rem_6rem] ${
        isSelected ? "bg-primary/5" : "hover:bg-secondary/30"
      }`}
    >
      <div
        className={`hidden items-center justify-center lg:flex ${
          isSelected ? "" : "opacity-0 group-hover/row:opacity-100"
        }`}
        onClick={(event) => {
          event.stopPropagation();
        }}
      >
        <input
          type="checkbox"
          checked={isSelected}
          onChange={() => toggleSelect(story.id)}
          onClick={(event) => event.stopPropagation()}
          aria-label={`Select ${story.title}`}
          className="h-3.5 w-3.5"
        />
      </div>
      <div className="hidden lg:inline-flex">
        <EditableStatusBadge story={story} />
      </div>
      <div className="min-w-0">
        <div className="flex min-w-0 items-center gap-2">
          <span className="shrink-0 lg:hidden">
            <EditableStatusBadge story={story} />
          </span>
          <span className="truncate font-medium text-foreground">{story.title}</span>
          {story.tags.slice(0, 2).map((tag) => (
            <span
              key={tag}
              className="hidden max-w-24 shrink-0 truncate rounded-[6px] bg-secondary px-1.5 py-0.5 text-[10px] text-muted-foreground md:inline"
            >
              {tag}
            </span>
          ))}
        </div>
        {story.description && (
          <p className="mt-0.5 truncate text-xs text-muted-foreground">{story.description}</p>
        )}
      </div>
      <div className="hidden shrink-0 lg:inline-flex">
        <EditableTypeBadge story={story} />
      </div>
      <div className="shrink-0">
        <EditablePriorityBadge story={story} />
      </div>
      <span className="hidden shrink-0 text-xs text-muted-foreground lg:block">{taskCount}</span>
      <span className="hidden shrink-0 text-xs text-muted-foreground lg:block">{contextCount}</span>
      <span className="hidden shrink-0 text-right text-xs text-muted-foreground lg:block">
        {new Date(story.updated_at).toLocaleDateString("zh-CN")}
      </span>
    </div>
  );
}

export function StoryListView({
  stories,
  taskCountByStoryId,
  onOpenStory,
  projectId,
}: StoryListViewProps) {
  const search = useStoryViewStore((s) => s.search);
  const scope = useStoryViewStore((s) => s.scope);
  const statusFilter = useStoryViewStore((s) => s.statusFilter);
  const priorityFilter = useStoryViewStore((s) => s.priorityFilter);
  const typeFilter = useStoryViewStore((s) => s.typeFilter);
  const sort = useStoryViewStore((s) => s.sort);
  const viewMode = useStoryViewStore((s) => s.viewMode);
  const isCreateOpen = useStoryViewStore((s) => s.isCreateOpen);
  const createInitialStatus = useStoryViewStore((s) => s.createInitialStatus);
  const openCreate = useStoryViewStore((s) => s.openCreate);
  const closeCreate = useStoryViewStore((s) => s.closeCreate);
  const clearFilters = useStoryViewStore((s) => s.clearFilters);

  const filtered = useMemo(
    () =>
      selectFilteredStories(stories, {
        search,
        scope,
        statusFilter,
        priorityFilter,
        typeFilter,
        sort,
      }),
    [stories, search, scope, statusFilter, priorityFilter, typeFilter, sort],
  );

  const activeStories = stories.filter(
    (story) => story.status !== "completed" && story.status !== "cancelled",
  ).length;
  const filterCount = activeFilterCount({
    search,
    scope,
    statusFilter,
    priorityFilter,
    typeFilter,
    sort,
  });
  const hasFilters = filterCount > 0;

  const handleClearAll = () => {
    clearFilters();
  };

  return (
    <>
      <div className="flex h-full flex-col overflow-hidden">
        <header className="shrink-0 border-b border-border bg-background">
          <div className="flex h-12 items-center justify-between px-4">
            <div className="flex min-w-0 items-center gap-3">
              <span className="agentdash-panel-header-tag">Story</span>
              <div className="min-w-0">
                <h2 className="truncate text-sm font-semibold text-foreground">Stories</h2>
                <p className="text-xs text-muted-foreground">
                  {filtered.length} visible · {stories.length} total · {activeStories} active
                </p>
              </div>
            </div>
            <CreateButton entity="Story" onClick={() => openCreate()} />
          </div>

          <StoryToolbar filterCount={filterCount} hasFilters={hasFilters} />
        </header>

        <div className="flex-1 overflow-hidden bg-background p-3">
          {filtered.length === 0 ? (
            <EmptyState className="flex h-full flex-col items-center justify-center gap-3">
              <div>
                <p className="font-medium text-foreground">
                  {hasFilters ? "没有匹配的 Story" : "当前 Project 暂无 Story"}
                </p>
                <p className="mt-1 text-xs text-muted-foreground">
                  {hasFilters
                    ? "调整搜索或筛选条件后再试。"
                    : "创建第一个 Story 后，可以在这里拆分 Task 并跟进 Agent 执行。"}
                </p>
              </div>
              {hasFilters ? (
                <Button type="button" variant="secondary" size="sm" onClick={handleClearAll}>
                  清空筛选
                </Button>
              ) : (
                <CreateButton entity="Story" onClick={() => openCreate()} />
              )}
            </EmptyState>
          ) : viewMode === "board" ? (
            <StoryBoard
              stories={filtered}
              taskCountByStoryId={taskCountByStoryId}
              projectId={projectId}
              onOpenStory={onOpenStory}
              onOpenFullCreate={(status) => openCreate(status)}
            />
          ) : (
            <div className="flex h-full flex-col overflow-hidden rounded-[8px] border border-border bg-background">
              <SectionTitle
                title="Story 列表"
                subtitle="点击任一行打开详情；Cmd/Ctrl+Click 多选"
                badge={`${filtered.length}`}
                sticky
              />
              <div className="min-h-0 flex-1 overflow-y-auto">
                <div className="hidden grid-cols-[2rem_8rem_minmax(0,1fr)_5rem_5rem_5rem_5rem_6rem] gap-3 border-b border-border bg-secondary/20 px-4 py-2 text-[10px] font-medium text-muted-foreground lg:grid">
                  <span></span>
                  <span>status</span>
                  <span>story</span>
                  <span>type</span>
                  <span>priority</span>
                  <span>task</span>
                  <span>context</span>
                  <span className="text-right">updated</span>
                </div>
                {filtered.map((story) => (
                  <StoryListRow
                    key={story.id}
                    story={story}
                    taskCount={taskCountByStoryId[story.id] ?? 0}
                    onOpenStory={onOpenStory}
                  />
                ))}
              </div>
            </div>
          )}
        </div>
      </div>

      <CreateStoryDrawer
        initialStatus={createInitialStatus}
        open={isCreateOpen}
        projectId={projectId}
        onClose={closeCreate}
      />
    </>
  );
}
