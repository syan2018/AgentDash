import { useMemo, useState } from "react";
import type { Story, StoryStatus } from "../../types";
import { StoryCard } from "./story-card";

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
}

export function StoryListView({ stories, taskCountByStoryId, onOpenStory }: StoryListViewProps) {
  const [search, setSearch] = useState("");

  const filtered = useMemo(() => {
    if (!search.trim()) return stories;
    const keyword = search.trim().toLowerCase();
    return stories.filter((story) => story.title.toLowerCase().includes(keyword));
  }, [stories, search]);

  return (
    <div className="flex h-full flex-col overflow-hidden">
      <header className="flex h-14 shrink-0 items-center justify-between border-b border-border bg-card px-6">
        <div>
          <h2 className="text-sm font-semibold tracking-tight text-foreground">Story 列表</h2>
          <p className="text-xs text-muted-foreground">{stories.length} 个 Story</p>
        </div>
        <input
          value={search}
          onChange={(event) => setSearch(event.target.value)}
          placeholder="搜索 Story..."
          className="h-8 w-56 rounded-md border border-border bg-background px-3 text-sm outline-none ring-ring focus:ring-1"
        />
      </header>

      <div className="flex-1 overflow-y-auto">
        {statusGroups.map((group) => {
          const groupItems = filtered.filter((story) => story.status === group.key);
          if (groupItems.length === 0) return null;
          return (
            <section key={group.key} className="border-b border-border last:border-b-0">
              <div className="flex items-center gap-2 bg-secondary/40 px-6 py-2">
                <span className={`h-2 w-2 rounded-full ${group.dotClass}`} />
                <h3 className="text-xs font-semibold uppercase tracking-wide text-muted-foreground">{group.label}</h3>
                <span className="text-xs text-muted-foreground">({groupItems.length})</span>
              </div>
              <div className="space-y-2 px-6 py-3">
                {groupItems.map((story) => (
                  <StoryCard
                    key={story.id}
                    story={story}
                    taskCount={taskCountByStoryId[story.id] ?? story.taskIds.length}
                    onClick={() => onOpenStory(story)}
                  />
                ))}
              </div>
            </section>
          );
        })}
      </div>
    </div>
  );
}
