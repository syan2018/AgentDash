import { useMemo, useState } from "react";
import type { Story, StoryStatus } from "../../types";
import { StoryCard } from "./story-card";
import { useStoryStore } from "../../stores/storyStore";
import { DetailPanel, DetailSection } from "../../components/ui/detail-panel";

const statusGroups: Array<{ key: StoryStatus; label: string; dotClass: string }> = [
  { key: "running", label: "执行中", dotClass: "bg-primary" },
  { key: "review", label: "待验收", dotClass: "bg-warning" },
  { key: "ready", label: "就绪", dotClass: "bg-info" },
  { key: "draft", label: "草稿", dotClass: "bg-muted-foreground" },
  { key: "completed", label: "已完成", dotClass: "bg-success" },
  { key: "failed", label: "失败", dotClass: "bg-destructive" },
  { key: "cancelled", label: "已取消", dotClass: "bg-muted-foreground" },
];

interface StoryListViewProps {
  stories: Story[];
  taskCountByStoryId: Record<string, number>;
  onOpenStory: (story: Story) => void;
  projectId: string;
  backendId: string;
}

interface CreateStoryDrawerProps {
  open: boolean;
  projectId: string;
  backendId: string;
  onClose: () => void;
}

function CreateStoryDrawer({ open, projectId, backendId, onClose }: CreateStoryDrawerProps) {
  const { createStory, error } = useStoryStore();
  const [title, setTitle] = useState("");
  const [description, setDescription] = useState("");
  const [formMessage, setFormMessage] = useState<string | null>(null);

  const handleCreate = async () => {
    const trimmedTitle = title.trim();
    if (!trimmedTitle) {
      setFormMessage("Story 标题不能为空");
      return;
    }
    if (!backendId) {
      setFormMessage("当前项目缺少 backend_id，无法创建 Story");
      return;
    }

    const created = await createStory(
      projectId,
      backendId,
      trimmedTitle,
      description.trim() || undefined,
    );
    if (!created) return;
    setTitle("");
    setDescription("");
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
            className="w-full rounded-md border border-border bg-background px-3 py-1.5 text-sm outline-none ring-ring focus:ring-1"
          />
          <textarea
            value={description}
            onChange={(e) => setDescription(e.target.value)}
            rows={4}
            placeholder="描述（可选）"
            className="w-full rounded-md border border-border bg-background px-3 py-1.5 text-sm outline-none ring-ring focus:ring-1"
          />
        </DetailSection>

        {(formMessage || error) && (
          <p className="text-xs text-destructive">{formMessage || error}</p>
        )}

        <div className="flex justify-end border-t border-border pt-3">
          <button
            type="button"
            onClick={() => void handleCreate()}
            disabled={!title.trim() || !backendId}
            className="rounded-md bg-primary px-4 py-1.5 text-sm text-primary-foreground disabled:opacity-50"
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
  backendId,
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
        <header className="flex h-14 shrink-0 items-center justify-between border-b border-border bg-card px-6">
          <div>
            <h2 className="text-sm font-semibold tracking-tight text-foreground">Story 列表</h2>
            <p className="text-xs text-muted-foreground">{stories.length} 个 Story</p>
          </div>
          <div className="flex items-center gap-2">
            <input
              value={search}
              onChange={(event) => setSearch(event.target.value)}
              placeholder="搜索 Story..."
              className="h-8 w-56 rounded-md border border-border bg-background px-3 text-sm outline-none ring-ring focus:ring-1"
            />
            <button
              type="button"
              onClick={() => setIsCreateOpen(true)}
              className="h-8 rounded-md bg-primary px-3 text-sm text-primary-foreground hover:bg-primary/90"
            >
              + 创建
            </button>
          </div>
        </header>

        <div className="flex-1 overflow-y-auto">
          {statusGroups.map((group) => {
            const groupItems = filtered.filter((story) => story.status === group.key);
            if (groupItems.length === 0) return null;
            return (
              <section key={group.key} className="border-b border-border last:border-b-0">
                <div className="flex items-center gap-2 bg-secondary/40 px-6 py-2">
                  <span className={`h-2 w-2 rounded-full ${group.dotClass}`} />
                  <h3 className="text-xs font-semibold uppercase tracking-wide text-muted-foreground">
                    {group.label}
                  </h3>
                  <span className="text-xs text-muted-foreground">({groupItems.length})</span>
                </div>
                <div className="space-y-2 px-6 py-3">
                  {groupItems.map((story) => (
                    <StoryCard
                      key={story.id}
                      story={story}
                      taskCount={taskCountByStoryId[story.id] ?? 0}
                      onClick={() => onOpenStory(story)}
                    />
                  ))}
                </div>
              </section>
            );
          })}
        </div>
      </div>

      <CreateStoryDrawer
        open={isCreateOpen}
        projectId={projectId}
        backendId={backendId}
        onClose={() => setIsCreateOpen(false)}
      />
    </>
  );
}
