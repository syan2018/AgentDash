import { useEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import type { Story } from "../../types";
import { useSortable } from "@dnd-kit/sortable";
import { CSS } from "@dnd-kit/utilities";
import {
  StoryPriorityToken,
  StoryTypeToken,
} from "../../components/ui/status-badge";
import {
  EditablePriorityBadge,
  EditableStatusBadge,
  EditableTypeBadge,
} from "./story-edit-badges";
import { useStoryViewStore } from "../../stores/storyViewStore";

interface StoryCardProps {
  story: Story;
  onClick: () => void;
  isDragging?: boolean;
  selectable?: boolean;
  showHoverDescription?: boolean;
  inert?: boolean;
}

function formatStoryKey(id: string): string {
  return `ST-${id.slice(0, 4).toUpperCase()}`;
}

export function StoryCard({
  story,
  onClick,
  isDragging,
  selectable = false,
  showHoverDescription = false,
  inert = false,
}: StoryCardProps) {
  const contextCount =
    story.context.source_refs.length +
    story.context.context_containers.length +
    story.context.disabled_container_ids.length +
    (story.context.session_composition ? 1 : 0);

  const setFocusedStory = useStoryViewStore((s) => s.setFocusedStory);
  const toggleSelect = useStoryViewStore((s) => s.toggleSelect);
  const isSelected = useStoryViewStore((s) => s.selectedIds.has(story.id));

  const [previewPos, setPreviewPos] = useState<{
    top: number;
    left: number;
    placement: "right" | "below";
    width: number;
    maxHeight: number;
  } | null>(null);
  const hoverTimerRef = useRef<number | null>(null);
  const cardRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    return () => {
      if (hoverTimerRef.current != null) window.clearTimeout(hoverTimerRef.current);
    };
  }, []);

  const handleMouseEnter = () => {
    if (!showHoverDescription || !story.description) return;
    if (hoverTimerRef.current != null) window.clearTimeout(hoverTimerRef.current);
    hoverTimerRef.current = window.setTimeout(() => {
      if (!cardRef.current) return;
      const rect = cardRef.current.getBoundingClientRect();
      const preferredWidth = 320;
      const minHeight = 160;
      const gap = 8;
      const margin = 12;
      const viewportH = window.innerHeight;
      const spaceRight = window.innerWidth - rect.right - margin;
      if (spaceRight >= preferredWidth + gap) {
        const top = Math.max(margin, Math.min(rect.top, viewportH - minHeight - margin));
        setPreviewPos({
          top,
          left: rect.right + gap,
          placement: "right",
          width: preferredWidth,
          maxHeight: viewportH - top - margin,
        });
      } else {
        const left = Math.max(margin, Math.min(rect.left, window.innerWidth - preferredWidth - margin));
        const top = Math.max(margin, Math.min(rect.bottom + gap, viewportH - minHeight - margin));
        setPreviewPos({
          top,
          left,
          placement: "below",
          width: preferredWidth,
          maxHeight: viewportH - top - margin,
        });
      }
    }, 400);
  };
  const handleMouseLeave = () => {
    if (hoverTimerRef.current != null) {
      window.clearTimeout(hoverTimerRef.current);
      hoverTimerRef.current = null;
    }
    setPreviewPos(null);
  };

  const handleCardClick = (event: React.MouseEvent) => {
    if (inert) return;
    if (selectable && (event.metaKey || event.ctrlKey)) {
      event.preventDefault();
      event.stopPropagation();
      toggleSelect(story.id);
      return;
    }
    onClick();
  };

  return (
    <div
      ref={cardRef}
      onClick={handleCardClick}
      onMouseEnter={handleMouseEnter}
      onMouseLeave={handleMouseLeave}
      onFocus={() => setFocusedStory(story.id)}
      onBlur={() => setFocusedStory(null)}
      tabIndex={inert ? -1 : 0}
      data-story-card-id={story.id}
      className={`group/card relative w-full cursor-pointer rounded-[8px] border bg-card px-2.5 py-3 text-left shadow-[0_3px_6px_-2px_rgba(0,0,0,0.02),0_1px_1px_0_rgba(0,0,0,0.04)] outline-none transition-colors focus-visible:ring-2 focus-visible:ring-primary/40 ${
        isSelected
          ? "border-primary/50 bg-accent/50"
          : "border-border hover:border-primary/25 hover:bg-accent/40"
      } ${isDragging ? "ring-2 ring-primary/20" : ""}`}
    >
      <div className="flex items-center justify-between gap-2">
        <p className="font-mono text-[11px] text-muted-foreground">{formatStoryKey(story.id)}</p>
        <EditableStatusBadge story={story} align="end" />
      </div>

      <div className="mt-1 min-w-0">
        <p className="line-clamp-2 text-sm font-medium leading-snug text-foreground group-hover/card:text-foreground">
          {story.title}
        </p>
        {story.description && (
          <p className="mt-1 line-clamp-1 text-xs leading-5 text-muted-foreground">
            {story.description}
          </p>
        )}
      </div>

      {(story.tags.length > 0 || contextCount > 0) && (
        <div className="mt-2 flex flex-wrap items-center gap-1.5">
          {story.tags.slice(0, 2).map((tag) => (
            <span
              key={tag}
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
        {inert ? (
          <>
            <StoryPriorityToken priority={story.priority} />
            <StoryTypeToken type={story.story_type} />
          </>
        ) : (
          <>
            <EditablePriorityBadge story={story} />
            <EditableTypeBadge story={story} />
          </>
        )}
        <span className="ml-auto">{new Date(story.updated_at).toLocaleDateString("zh-CN")}</span>
      </div>

      {previewPos &&
        story.description &&
        createPortal(
          <div
            role="tooltip"
            style={{
              position: "fixed",
              top: previewPos.top,
              left: previewPos.left,
              width: previewPos.width,
              maxHeight: previewPos.maxHeight,
            }}
            className="pointer-events-none z-[80] overflow-y-auto rounded-[8px] border border-border bg-card p-3 text-xs leading-5 text-foreground shadow-xl shadow-foreground/15"
          >
            <p className="mb-1.5 text-[10px] font-medium uppercase tracking-wide text-muted-foreground">
              {story.title}
            </p>
            <p className="whitespace-pre-wrap break-words">{story.description}</p>
          </div>,
          document.body,
        )}
    </div>
  );
}

interface SortableStoryCardProps {
  story: Story;
  onClick: () => void;
  selectable?: boolean;
  showHoverDescription?: boolean;
}

export function SortableStoryCard({
  story,
  onClick,
  selectable,
  showHoverDescription,
}: SortableStoryCardProps) {
  const { attributes, listeners, setNodeRef, transform, transition, isDragging } =
    useSortable({ id: story.id });

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
        onClick={onClick}
        isDragging={isDragging}
        selectable={selectable}
        showHoverDescription={showHoverDescription}
      />
    </div>
  );
}
