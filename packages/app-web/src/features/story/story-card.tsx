import type { Story } from "../../types";
import { useSortable } from "@dnd-kit/sortable";
import { CSS } from "@dnd-kit/utilities";
import { StoryPriorityToken, StoryStatusBadge, StoryTypeToken } from "../../components/ui/status-badge";

interface StoryCardProps {
  story: Story;
  taskCount: number;
  onClick: () => void;
  isDragging?: boolean;
}

function formatStoryKey(id: string): string {
  return `ST-${id.slice(0, 4).toUpperCase()}`;
}

export function StoryCard({ story, taskCount, onClick, isDragging }: StoryCardProps) {
  const contextCount =
    story.context.source_refs.length +
    story.context.context_containers.length +
    story.context.disabled_container_ids.length +
    (story.context.session_composition ? 1 : 0);

  return (
    <div
      onClick={onClick}
      className={`group/card w-full cursor-pointer rounded-[8px] border border-border bg-card px-2.5 py-3 text-left shadow-[0_3px_6px_-2px_rgba(0,0,0,0.02),0_1px_1px_0_rgba(0,0,0,0.04)] transition-colors hover:border-primary/25 hover:bg-accent/40 ${
        isDragging ? "ring-2 ring-primary/20" : ""
      }`}
    >
      <div className="flex items-center justify-between gap-2">
        <p className="font-mono text-[11px] text-muted-foreground">{formatStoryKey(story.id)}</p>
        <StoryStatusBadge status={story.status} />
      </div>

      <div className="mt-1 min-w-0">
        <p className="line-clamp-2 text-sm font-medium leading-snug text-foreground group-hover/card:text-foreground">{story.title}</p>
        {story.description && (
          <p className="mt-1 line-clamp-1 text-xs leading-5 text-muted-foreground">{story.description}</p>
        )}
      </div>

      {(story.tags.length > 0 || contextCount > 0) && (
        <div className="mt-2 flex flex-wrap items-center gap-1.5">
          {story.tags.slice(0, 2).map((tag, index) => (
            <span
              key={index}
              className="inline-flex max-w-[92px] truncate rounded-[6px] bg-secondary px-1.5 py-0.5 text-[10px] text-muted-foreground"
              title={tag}
            >
              {tag}
            </span>
          ))}
          {story.tags.length > 2 && (
            <span className="text-[10px] text-muted-foreground">+{story.tags.length - 2}</span>
          )}
          {contextCount > 0 && (
            <span className="inline-flex rounded-[6px] bg-secondary px-1.5 py-0.5 text-[10px] text-muted-foreground">
              {contextCount} Context
            </span>
          )}
        </div>
      )}

      <div className="mt-3 flex items-center gap-2 text-[11px] text-muted-foreground">
        <StoryPriorityToken priority={story.priority} />
        <StoryTypeToken type={story.story_type} />
        <span>{taskCount} Task</span>
        <span className="ml-auto">{new Date(story.updated_at).toLocaleDateString("zh-CN")}</span>
      </div>
    </div>
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
    opacity: isDragging ? 0.42 : 1,
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
