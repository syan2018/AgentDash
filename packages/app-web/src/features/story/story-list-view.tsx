import { useMemo, useState } from "react";
import type { Story, StoryPriority, StoryStatus, StoryType } from "../../types";
import { StoryBoard } from "./story-board";
import { useStoryStore } from "../../stores/storyStore";
import { Button, DetailPanel, DetailSection, EmptyState, Field, SectionTitle, Select, Textarea, TextInput } from "@agentdash/ui";
import { StoryPriorityBadge, StoryPriorityToken, StoryStatusBadge, StoryTypeBadge, StoryTypeToken } from "../../components/ui/status-badge";

interface StoryListViewProps {
  stories: Story[];
  taskCountByStoryId: Record<string, number>;
  onOpenStory: (story: Story) => void;
  projectId: string;
}

type StoryViewMode = "board" | "list";
type StorySortKey = "priority" | "updated" | "title";
type StoryScope = "all" | "active" | "done";

const statusOptions: { value: StoryStatus; label: string }[] = [
  { value: "draft", label: "draft" },
  { value: "ready", label: "ready" },
  { value: "running", label: "running" },
  { value: "review", label: "review" },
  { value: "completed", label: "completed" },
  { value: "failed", label: "failed" },
  { value: "cancelled", label: "cancelled" },
];

const priorityOptions: { value: StoryPriority; label: string }[] = [
  { value: "p0", label: "p0" },
  { value: "p1", label: "p1" },
  { value: "p2", label: "p2" },
  { value: "p3", label: "p3" },
];

const storyTypeOptions: { value: StoryType; label: string; icon: string }[] = [
  { value: "feature", label: "feature", icon: "FEAT" },
  { value: "bugfix", label: "bugfix", icon: "BUG" },
  { value: "refactor", label: "refactor", icon: "REF" },
  { value: "docs", label: "docs", icon: "DOC" },
  { value: "test", label: "test", icon: "TEST" },
  { value: "other", label: "other", icon: "OTHR" },
];

const priorityWeight: Record<StoryPriority, number> = {
  p0: 0,
  p1: 1,
  p2: 2,
  p3: 3,
};

const scopeOptions: { value: StoryScope; label: string }[] = [
  { value: "all", label: "All" },
  { value: "active", label: "Active" },
  { value: "done", label: "Done" },
];

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

function CreateStoryDrawer({ initialStatus, open, projectId, onClose }: CreateStoryDrawerProps) {
  const { createStory, error } = useStoryStore();
  const [title, setTitle] = useState("");
  const [description, setDescription] = useState("");
  const [status, setStatus] = useState<StoryStatus>(initialStatus);
  const [priority, setPriority] = useState<StoryPriority>("p2");
  const [storyType, setStoryType] = useState<StoryType>("feature");
  const [tags, setTags] = useState("");
  const [formMessage, setFormMessage] = useState<string | null>(null);

  const handleCreate = async () => {
    const trimmedTitle = title.trim();
    if (!trimmedTitle) {
      setFormMessage("Story 标题不能为空");
      return;
    }

    const created = await createStory(
      projectId,
      trimmedTitle,
      description.trim() || undefined,
      {
        status,
        priority,
        story_type: storyType,
        tags: parseTags(tags),
      },
    );
    if (!created) return;

    setTitle("");
    setDescription("");
    setStatus(initialStatus);
    setPriority("p2");
    setStoryType("feature");
    setTags("");
    setFormMessage(null);
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
      <div className="space-y-4 p-5">
        <DetailSection title="基础信息">
          <Field label="Story 标题">
            <TextInput
              value={title}
              onChange={(e) => setTitle(e.target.value)}
              placeholder="用户可感知的一段交付目标"
            />
          </Field>
          <Field label="描述">
            <Textarea
              value={description}
              onChange={(e) => setDescription(e.target.value)}
              rows={4}
              placeholder="补充背景、验收口径或实现边界"
            />
          </Field>
        </DetailSection>

        <DetailSection title="状态与属性">
          <div className="grid grid-cols-3 gap-3">
            <Field label="status">
              <Select
                value={status}
                onChange={(e) => setStatus(e.target.value as StoryStatus)}
              >
                {statusOptions.map((opt) => (
                  <option key={opt.value} value={opt.value}>
                    {opt.label}
                  </option>
                ))}
              </Select>
              <div className="mt-2">
                <StoryStatusBadge status={status} />
              </div>
            </Field>

            <Field label="priority">
              <Select
                value={priority}
                onChange={(e) => setPriority(e.target.value as StoryPriority)}
              >
                {priorityOptions.map((opt) => (
                  <option key={opt.value} value={opt.value}>
                    {opt.label}
                  </option>
                ))}
              </Select>
              <div className="mt-2">
                <StoryPriorityBadge priority={priority} showLabel />
              </div>
            </Field>

            <Field label="type">
              <Select
                value={storyType}
                onChange={(e) => setStoryType(e.target.value as StoryType)}
              >
                {storyTypeOptions.map((opt) => (
                  <option key={opt.value} value={opt.value}>
                    {opt.icon} {opt.label}
                  </option>
                ))}
              </Select>
              <div className="mt-2">
                <StoryTypeBadge type={storyType} />
              </div>
            </Field>
          </div>
        </DetailSection>

        <DetailSection title="标签">
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
                  className="inline-flex items-center rounded-[8px] border border-border bg-background px-2 py-0.5 text-xs text-muted-foreground"
                >
                  {tag}
                </span>
              ))}
            </div>
          )}
        </DetailSection>

        {(formMessage || error) && (
          <p className="text-xs text-destructive">{formMessage || error}</p>
        )}

        <div className="flex justify-end border-t border-border pt-3">
          <Button
            type="button"
            variant="primary"
            size="sm"
            onClick={() => void handleCreate()}
            disabled={!title.trim()}
          >
            创建 Story
          </Button>
        </div>
      </div>
    </DetailPanel>
  );
}

function StoryListRow({
  story,
  taskCount,
  onOpenStory,
}: {
  story: Story;
  taskCount: number;
  onOpenStory: (story: Story) => void;
}) {
  const contextCount =
    story.context.source_refs.length +
    story.context.context_containers.length +
    story.context.disabled_container_ids.length +
    (story.context.session_composition ? 1 : 0);

  return (
    <button
      type="button"
      onClick={() => onOpenStory(story)}
      className="group grid min-h-12 w-full grid-cols-[minmax(0,1fr)] items-center gap-3 border-b border-border px-4 py-2 text-left text-sm transition-colors hover:bg-secondary/30 lg:grid-cols-[8rem_minmax(0,1fr)_5rem_5rem_5rem_5rem_6rem]"
    >
      <StoryStatusBadge status={story.status} className="hidden lg:inline-flex" />
      <div className="min-w-0">
        <div className="flex min-w-0 items-center gap-2">
          <StoryStatusBadge status={story.status} className="shrink-0 lg:hidden" />
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
      <StoryTypeToken type={story.story_type} className="hidden shrink-0 lg:inline-flex" />
      <StoryPriorityToken priority={story.priority} className="shrink-0" />
      <span className="hidden shrink-0 text-xs text-muted-foreground lg:block">{taskCount}</span>
      <span className="hidden shrink-0 text-xs text-muted-foreground lg:block">{contextCount}</span>
      <span className="hidden shrink-0 text-right text-xs text-muted-foreground lg:block">
        {new Date(story.updated_at).toLocaleDateString("zh-CN")}
      </span>
    </button>
  );
}

function ChevronIcon() {
  return (
    <svg className="h-3.5 w-3.5 text-muted-foreground" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeLinecap="round" strokeLinejoin="round" strokeWidth={2}>
      <path d="m6 9 6 6 6-6" />
    </svg>
  );
}

function SearchIcon() {
  return (
    <svg className="h-3.5 w-3.5 text-muted-foreground" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeLinecap="round" strokeLinejoin="round" strokeWidth={2}>
      <circle cx="11" cy="11" r="8" />
      <path d="m21 21-4.3-4.3" />
    </svg>
  );
}

function BoardIcon() {
  return (
    <svg className="h-3.5 w-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeLinecap="round" strokeLinejoin="round" strokeWidth={2}>
      <rect x="3" y="4" width="5" height="16" rx="1.5" />
      <rect x="10" y="4" width="5" height="16" rx="1.5" />
      <rect x="17" y="4" width="4" height="16" rx="1.5" />
    </svg>
  );
}

function ListIcon() {
  return (
    <svg className="h-3.5 w-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeLinecap="round" strokeLinejoin="round" strokeWidth={2}>
      <path d="M8 6h13" />
      <path d="M8 12h13" />
      <path d="M8 18h13" />
      <path d="M3 6h.01" />
      <path d="M3 12h.01" />
      <path d="M3 18h.01" />
    </svg>
  );
}

function ToolbarSelect<T extends string>({
  label,
  value,
  onChange,
  options,
}: {
  label: string;
  value: T | "all";
  onChange: (value: T | "all") => void;
  options: { value: T; label: string }[];
}) {
  const selectedLabel = value === "all" ? "all" : options.find((option) => option.value === value)?.label ?? value;
  const active = value !== "all";

  return (
    <label
      className={`relative inline-flex h-8 cursor-pointer items-center gap-1.5 rounded-[8px] border px-2.5 text-xs transition-colors ${
        active
          ? "border-primary/30 bg-primary/5 text-foreground"
          : "border-border bg-background text-muted-foreground hover:bg-secondary/30 hover:text-foreground"
      }`}
    >
      <span className="font-medium">{label}</span>
      <span className="font-mono text-[11px]">{selectedLabel}</span>
      <ChevronIcon />
      <select
        value={value}
        onChange={(event) => onChange(event.target.value as T | "all")}
        className="absolute inset-0 cursor-pointer opacity-0"
      >
        <option value="all">all</option>
        {options.map((option) => (
          <option key={option.value} value={option.value}>
            {option.label}
          </option>
        ))}
      </select>
    </label>
  );
}

function SortSelect({
  value,
  onChange,
}: {
  value: StorySortKey;
  onChange: (value: StorySortKey) => void;
}) {
  const options: { value: StorySortKey; label: string }[] = [
    { value: "priority", label: "priority" },
    { value: "updated", label: "updated" },
    { value: "title", label: "title" },
  ];
  const selectedLabel = options.find((option) => option.value === value)?.label ?? value;

  return (
    <label className="relative inline-flex h-8 cursor-pointer items-center gap-1.5 rounded-[8px] border border-border bg-background px-2.5 text-xs text-muted-foreground transition-colors hover:bg-secondary/30 hover:text-foreground">
      <span className="font-medium">Sort</span>
      <span className="font-mono text-[11px]">{selectedLabel}</span>
      <ChevronIcon />
      <select
        value={value}
        onChange={(event) => onChange(event.target.value as StorySortKey)}
        className="absolute inset-0 cursor-pointer opacity-0"
      >
        {options.map((option) => (
          <option key={option.value} value={option.value}>
            {option.label}
          </option>
        ))}
      </select>
    </label>
  );
}

export function StoryListView({
  stories,
  taskCountByStoryId,
  onOpenStory,
  projectId,
}: StoryListViewProps) {
  const [search, setSearch] = useState("");
  const [statusFilter, setStatusFilter] = useState<StoryStatus | "all">("all");
  const [priorityFilter, setPriorityFilter] = useState<StoryPriority | "all">("all");
  const [typeFilter, setTypeFilter] = useState<StoryType | "all">("all");
  const [scope, setScope] = useState<StoryScope>("all");
  const [sortBy, setSortBy] = useState<StorySortKey>("priority");
  const [viewMode, setViewMode] = useState<StoryViewMode>("board");
  const [isCreateOpen, setIsCreateOpen] = useState(false);
  const [createStatus, setCreateStatus] = useState<StoryStatus>("draft");

  const filtered = useMemo(() => {
    const keyword = search.trim().toLowerCase();
    const matchesKeyword = (story: Story) => {
      if (!keyword) return true;
      const haystack = [
        story.title,
        story.description ?? "",
        ...story.tags,
      ].join(" ").toLowerCase();
      return haystack.includes(keyword);
    };

    const result = stories.filter((story) => {
      if (scope === "active" && (story.status === "completed" || story.status === "cancelled")) return false;
      if (scope === "done" && story.status !== "completed" && story.status !== "cancelled") return false;
      if (!matchesKeyword(story)) return false;
      if (statusFilter !== "all" && story.status !== statusFilter) return false;
      if (priorityFilter !== "all" && story.priority !== priorityFilter) return false;
      if (typeFilter !== "all" && story.story_type !== typeFilter) return false;
      return true;
    });

    return [...result].sort((a, b) => {
      if (sortBy === "priority") {
        const byPriority = priorityWeight[a.priority] - priorityWeight[b.priority];
        if (byPriority !== 0) return byPriority;
        return new Date(b.updated_at).getTime() - new Date(a.updated_at).getTime();
      }
      if (sortBy === "title") return a.title.localeCompare(b.title);
      return new Date(b.updated_at).getTime() - new Date(a.updated_at).getTime();
    });
  }, [priorityFilter, scope, search, sortBy, statusFilter, stories, typeFilter]);

  const activeStories = stories.filter((story) => story.status !== "completed" && story.status !== "cancelled").length;
  const hasFilters = Boolean(search.trim()) || statusFilter !== "all" || priorityFilter !== "all" || typeFilter !== "all";
  const filterCount = [search.trim(), statusFilter !== "all", priorityFilter !== "all", typeFilter !== "all"].filter(Boolean).length;

  const openCreate = (status?: StoryStatus) => {
    setCreateStatus(status ?? (statusFilter === "all" ? "draft" : statusFilter));
    setIsCreateOpen(true);
  };

  const clearFilters = () => {
    setSearch("");
    setStatusFilter("all");
    setPriorityFilter("all");
    setTypeFilter("all");
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
                <p className="text-xs text-muted-foreground">{filtered.length} visible · {stories.length} total · {activeStories} active</p>
              </div>
            </div>
            <Button type="button" variant="primary" size="sm" onClick={() => openCreate()}>
              New Story
            </Button>
          </div>

          <div className="flex min-h-12 items-center justify-between gap-3 border-t border-border px-4 py-2">
            <div className="flex min-w-0 items-center gap-1">
              {scopeOptions.map((option) => (
                <button
                  key={option.value}
                  type="button"
                  onClick={() => setScope(option.value)}
                  className={`h-8 rounded-[8px] border px-3 text-xs font-medium transition-colors ${
                    scope === option.value
                      ? "border-border bg-secondary/60 text-foreground"
                      : "border-border bg-background text-muted-foreground hover:bg-secondary/30 hover:text-foreground"
                  }`}
                >
                  {option.label}
                </button>
              ))}
            </div>

            <div className="flex min-w-0 flex-1 items-center justify-end gap-1.5">
              <label className="flex h-8 w-56 min-w-40 items-center gap-2 rounded-[8px] border border-border bg-background px-2.5 text-xs text-muted-foreground transition-colors focus-within:border-primary/30 focus-within:ring-1 focus-within:ring-ring">
                <SearchIcon />
                <input
                  value={search}
                  onChange={(event) => setSearch(event.target.value)}
                  placeholder="Search"
                  className="min-w-0 flex-1 bg-transparent text-sm text-foreground outline-none placeholder:text-muted-foreground"
                />
              </label>
              <ToolbarSelect label="status" value={statusFilter} onChange={setStatusFilter} options={statusOptions} />
              <ToolbarSelect label="priority" value={priorityFilter} onChange={setPriorityFilter} options={priorityOptions} />
              <ToolbarSelect label="type" value={typeFilter} onChange={setTypeFilter} options={storyTypeOptions} />
              {hasFilters && (
                <Button type="button" variant="ghost" size="sm" onClick={clearFilters}>
                  Clear {filterCount}
                </Button>
              )}
              <SortSelect value={sortBy} onChange={setSortBy} />
              <div className="flex h-8 rounded-[8px] border border-border bg-background p-0.5">
                <button
                  type="button"
                  title="Board"
                  onClick={() => setViewMode("board")}
                  className={`inline-flex h-6 w-7 items-center justify-center rounded-[6px] transition-colors ${
                    viewMode === "board" ? "bg-secondary text-foreground" : "text-muted-foreground hover:text-foreground"
                  }`}
                >
                  <BoardIcon />
                </button>
                <button
                  type="button"
                  title="List"
                  onClick={() => setViewMode("list")}
                  className={`inline-flex h-6 w-7 items-center justify-center rounded-[6px] transition-colors ${
                    viewMode === "list" ? "bg-secondary text-foreground" : "text-muted-foreground hover:text-foreground"
                  }`}
                >
                  <ListIcon />
                </button>
              </div>
            </div>
          </div>
        </header>

        <div className="flex-1 overflow-hidden bg-background p-3">
          {filtered.length === 0 ? (
            <EmptyState className="flex h-full flex-col items-center justify-center gap-3">
              <div>
                <p className="font-medium text-foreground">{hasFilters ? "没有匹配的 Story" : "当前 Project 暂无 Story"}</p>
                <p className="mt-1 text-xs text-muted-foreground">
                  {hasFilters ? "调整搜索或筛选条件后再试。" : "创建第一个 Story 后，可以在这里拆分 Task 并跟进 Agent 执行。"}
                </p>
              </div>
              {!hasFilters && (
                <Button type="button" variant="primary" size="sm" onClick={() => openCreate()}>
                  创建 Story
                </Button>
              )}
            </EmptyState>
          ) : viewMode === "board" ? (
            <StoryBoard
              stories={filtered}
              taskCountByStoryId={taskCountByStoryId}
              onCreateStory={openCreate}
              onOpenStory={onOpenStory}
            />
          ) : (
            <div className="flex h-full flex-col overflow-hidden rounded-[8px] border border-border bg-background">
              <SectionTitle
                title="Story 列表"
                subtitle="点击任一行打开详情工作台"
                badge={`${filtered.length}`}
                sticky
              />
              <div className="min-h-0 flex-1 overflow-y-auto">
                <div className="hidden grid-cols-[8rem_minmax(0,1fr)_5rem_5rem_5rem_5rem_6rem] gap-3 border-b border-border bg-secondary/20 px-4 py-2 text-[10px] font-medium text-muted-foreground lg:grid">
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
        key={`${createStatus}-${isCreateOpen ? "open" : "closed"}`}
        initialStatus={createStatus}
        open={isCreateOpen}
        projectId={projectId}
        onClose={() => setIsCreateOpen(false)}
      />
    </>
  );
}
