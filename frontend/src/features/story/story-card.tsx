import type { Story } from "../../types";
import { StoryStatusBadge } from "../../components/ui/status-badge";
import { useSortable } from "@dnd-kit/sortable";
import { CSS } from "@dnd-kit/utilities";

interface StoryCardProps {
  story: Story;
  taskCount: number;
  onClick: () => void;
  isDragging?: boolean;
}

export function StoryCard({ story, taskCount, onClick, isDragging }: StoryCardProps) {
  return (
    <button
      type="button"
      onClick={onClick}
      className={`w-full rounded-lg border border-border bg-card p-3 text-left transition-all hover:border-primary/50 hover:shadow-sm ${
        isDragging ? "rotate-2 scale-105 shadow-lg ring-2 ring-primary" : ""
      }`}
    >
      <div className="flex items-start justify-between gap-3">
        <div className="min-w-0 flex-1">
          <p className="truncate text-sm font-medium text-foreground">{story.title}</p>
          {story.description && (
            <p className="mt-1 line-clamp-2 text-xs text-muted-foreground">{story.description}</p>
          )}
        </div>
        <StoryStatusBadge status={story.status} />
      </div>
      <div className="mt-2 flex items-center justify-between text-xs text-muted-foreground">
        <span>{taskCount} 个任务</span>
        <span>{new Date(story.updated_at).toLocaleDateString("zh-CN")}</span>
      </div>
    </button>
  );
}

interface SortableStoryCardProps {
  story: Story;
  taskCount: number;
  onClick: () => void;
}

export function SortableStoryCard({ story, taskCount, onClick }: SortableStoryCardProps) {
  const {
    attributes,
    listeners,
    setNodeRef,
    transform,
    transition,
    isDragging,
  } = useSortable({ id: story.id });

  const style = {
    transform: CSS.Transform.toString(transform),
    transition,
    opacity: isDragging ? 0.5 : 1,
  };

  return (
    <div
      ref={setNodeRef}
      style={style}
      {...attributes}
      {...listeners}
      className="cursor-grab active:cursor-grabbing"
    >
      <StoryCard
        story={story}
        taskCount={taskCount}
        onClick={onClick}
        isDragging={isDragging}
      />
    </div>
  );
}
