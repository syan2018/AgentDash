import type { Story } from "../../types";
import { StoryStatusBadge, StoryPriorityBadge, StoryTypeBadge } from "../../components/ui/status-badge";
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
      {/* 顶部：类型和优先级 */}
      <div className="mb-2 flex items-center gap-1.5">
        <StoryTypeBadge type={story.story_type} />
        <StoryPriorityBadge priority={story.priority} showLabel />
        {story.tags.length > 0 && (
          <div className="ml-auto flex items-center gap-1">
            {story.tags.slice(0, 2).map((tag, index) => (
              <span
                key={index}
                className="inline-flex max-w-[60px] truncate rounded bg-muted px-1.5 py-0.5 text-[10px] text-muted-foreground"
                title={tag}
              >
                {tag}
              </span>
            ))}
            {story.tags.length > 2 && (
              <span className="text-[10px] text-muted-foreground">+{story.tags.length - 2}</span>
            )}
          </div>
        )}
      </div>

      {/* 标题和描述 */}
      <div className="flex items-start justify-between gap-3">
        <div className="min-w-0 flex-1">
          <p className="truncate text-sm font-medium text-foreground">{story.title}</p>
          {story.description && (
            <p className="mt-1 line-clamp-2 text-xs text-muted-foreground">{story.description}</p>
          )}
        </div>
        <StoryStatusBadge status={story.status} />
      </div>

      {/* 底部：任务数和时间 */}
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
