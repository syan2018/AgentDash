import type { Story } from "../../types";
import { StoryStatusBadge } from "../../components/ui/status-badge";

interface StoryCardProps {
  story: Story;
  taskCount: number;
  onClick: () => void;
}

export function StoryCard({ story, taskCount, onClick }: StoryCardProps) {
  return (
    <button
      type="button"
      onClick={onClick}
      className="w-full rounded-lg border border-border bg-card p-3 text-left transition-colors hover:bg-secondary/30"
    >
      <div className="flex items-start justify-between gap-3">
        <div className="min-w-0">
          <p className="truncate text-sm font-medium text-foreground">{story.title}</p>
          {story.description && <p className="mt-1 line-clamp-2 text-xs text-muted-foreground">{story.description}</p>}
        </div>
        <StoryStatusBadge status={story.status} />
      </div>
      <div className="mt-2 flex items-center justify-between text-xs text-muted-foreground">
        <span>{taskCount} 个任务</span>
        <span>{new Date(story.updatedAt).toLocaleDateString("zh-CN")}</span>
      </div>
    </button>
  );
}
