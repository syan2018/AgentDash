import { useMemo, useState } from "react";
import type { Story, StoryPriority, StoryType } from "../../types";
import { StoryBoard } from "./story-board";
import { useStoryStore } from "../../stores/storyStore";
import { DetailPanel, DetailSection } from "../../components/ui/detail-panel";
import { StoryPriorityBadge, StoryTypeBadge } from "../../components/ui/status-badge";


interface StoryListViewProps {
  stories: Story[];
  taskCountByStoryId: Record<string, number>;
  onOpenStory: (story: Story) => void;
  projectId: string;
}

// Story 优先级选项
const priorityOptions: { value: StoryPriority; label: string }[] = [
  { value: "p0", label: "P0 - 紧急" },
  { value: "p1", label: "P1 - 高" },
  { value: "p2", label: "P2 - 中" },
  { value: "p3", label: "P3 - 低" },
];

// Story 类型选项
const storyTypeOptions: { value: StoryType; label: string; icon: string }[] = [
  { value: "feature", label: "功能", icon: "✨" },
  { value: "bugfix", label: "缺陷", icon: "🐛" },
  { value: "refactor", label: "重构", icon: "♻️" },
  { value: "docs", label: "文档", icon: "📝" },
  { value: "test", label: "测试", icon: "🧪" },
  { value: "other", label: "其他", icon: "📦" },
];

interface CreateStoryDrawerProps {
  open: boolean;
  projectId: string;
  onClose: () => void;
}

function CreateStoryDrawer({ open, projectId, onClose }: CreateStoryDrawerProps) {
  const { createStory, error } = useStoryStore();
  const [title, setTitle] = useState("");
  const [description, setDescription] = useState("");
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

    const parsedTags = tags
      .split(",")
      .map((t) => t.trim())
      .filter((t) => t.length > 0);

    const created = await createStory(
      projectId,
      trimmedTitle,
      description.trim() || undefined,
      {
        priority,
        story_type: storyType,
        tags: parsedTags,
      },
    );
    if (!created) return;

    setTitle("");
    setDescription("");
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
      subtitle="创建后可在详情中继续编辑与拆分 Task"
      onClose={onClose}
      widthClassName="max-w-2xl"
    >
      <div className="space-y-4 p-5">
        <DetailSection title="基础信息">
          <input
            value={title}
            onChange={(e) => setTitle(e.target.value)}
            placeholder="Story 标题"
            className="agentdash-form-input"
          />
          <textarea
            value={description}
            onChange={(e) => setDescription(e.target.value)}
            rows={4}
            placeholder="描述（可选）"
            className="agentdash-form-textarea"
          />
        </DetailSection>

        <DetailSection title="优先级与类型">
          <div className="grid grid-cols-2 gap-3">
            <div>
              <label className="agentdash-form-label">优先级</label>
              <select
                value={priority}
                onChange={(e) => setPriority(e.target.value as StoryPriority)}
                className="agentdash-form-select"
              >
                {priorityOptions.map((opt) => (
                  <option key={opt.value} value={opt.value}>
                    {opt.label}
                  </option>
                ))}
              </select>
              <div className="mt-2">
                <StoryPriorityBadge priority={priority} showLabel />
              </div>
            </div>

            <div>
              <label className="agentdash-form-label">类型</label>
              <select
                value={storyType}
                onChange={(e) => setStoryType(e.target.value as StoryType)}
                className="agentdash-form-select"
              >
                {storyTypeOptions.map((opt) => (
                  <option key={opt.value} value={opt.value}>
                    {opt.icon} {opt.label}
                  </option>
                ))}
              </select>
              <div className="mt-2">
                <StoryTypeBadge type={storyType} />
              </div>
            </div>
          </div>
        </DetailSection>

        <DetailSection title="标签">
          <input
            value={tags}
            onChange={(e) => setTags(e.target.value)}
            placeholder="用逗号分隔，例如: frontend, api, urgent"
            className="agentdash-form-input"
          />
          {tags && (
            <div className="mt-2 flex flex-wrap gap-1">
              {tags
                .split(",")
                .map((t) => t.trim())
                .filter((t) => t.length > 0)
                .map((tag, index) => (
                  <span
                    key={index}
                    className="inline-flex items-center rounded-full border border-border bg-background px-2 py-0.5 text-xs text-muted-foreground"
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
          <button
            type="button"
            onClick={() => void handleCreate()}
            disabled={!title.trim()}
            className="agentdash-button-primary"
          >
            创建 Story
          </button>
        </div>
      </div>
    </DetailPanel>
  );
}

export function StoryListView({
  stories,
  taskCountByStoryId,
  onOpenStory,
  projectId,
}: StoryListViewProps) {
  const [search, setSearch] = useState("");
  const [isCreateOpen, setIsCreateOpen] = useState(false);

  const filtered = useMemo(() => {
    if (!search.trim()) return stories;
    const keyword = search.trim().toLowerCase();
    return stories.filter((story) => story.title.toLowerCase().includes(keyword));
  }, [stories, search]);

  return (
    <>
      <div className="flex h-full flex-col overflow-hidden">
        <header className="flex h-14 shrink-0 items-center justify-between border-b border-border bg-background px-6">
          <div className="flex items-center gap-2.5">
            <span className="inline-flex rounded-[8px] border border-border bg-secondary px-2 py-1 text-[11px] font-semibold uppercase tracking-[0.14em] text-muted-foreground">
              STORY
            </span>
            <div>
            <h2 className="text-sm font-semibold tracking-tight text-foreground">Story 列表</h2>
            <p className="text-xs text-muted-foreground">{stories.length} 个 Story</p>
            </div>
          </div>
          <div className="flex items-center gap-2">
            <input
              value={search}
              onChange={(event) => setSearch(event.target.value)}
              placeholder="搜索 Story..."
              className="h-9 w-56 rounded-[10px] border border-border bg-background px-3.5 text-sm outline-none ring-ring transition-colors focus:border-primary/30 focus:ring-1 focus:ring-ring/40"
            />
            <button
              type="button"
              onClick={() => setIsCreateOpen(true)}
              className="h-9 rounded-[10px] border border-primary bg-primary px-3.5 text-sm text-primary-foreground transition-colors hover:opacity-95"
            >
              + 创建
            </button>
          </div>
        </header>

        <div className="flex-1 overflow-hidden p-4 pt-3">
          <StoryBoard
            stories={filtered}
            taskCountByStoryId={taskCountByStoryId}
            onOpenStory={onOpenStory}
          />
        </div>
      </div>

      <CreateStoryDrawer
        open={isCreateOpen}
        projectId={projectId}
        onClose={() => setIsCreateOpen(false)}
      />
    </>
  );
}
