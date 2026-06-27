import { useMemo, useState, useEffect } from "react";
import type { Story, StoryStatus } from "../../types";
import {
  DndContext,
  DragOverlay,
  pointerWithin,
  KeyboardSensor,
  PointerSensor,
  useSensor,
  useSensors,
  type DragEndEvent,
  type DragStartEvent,
} from "@dnd-kit/core";
import {
  SortableContext,
  sortableKeyboardCoordinates,
  verticalListSortingStrategy,
} from "@dnd-kit/sortable";
import { StoryCard, SortableStoryCard } from "./story-card";
import { useStoryStore } from "../../stores/storyStore";
import { useDroppable } from "@dnd-kit/core";
import { StoryStatusToken } from "../../components/ui/status-badge";
import { useStoryViewStore } from "../../stores/storyViewStore";
import { StoryQuickAdd } from "./story-quick-add";

const storyStatusOrder: StoryStatus[] = [
  "created",
  "context_ready",
  "executing",
  "decomposed",
  "completed",
  "failed",
  "cancelled",
];

interface BoardColumn {
  status: StoryStatus;
}

const boardColumns: BoardColumn[] = storyStatusOrder.map((status) => ({ status }));

interface StoryBoardProps {
  stories: Story[];
  projectId: string;
  onOpenStory: (story: Story) => void;
  onOpenFullCreate?: (status: StoryStatus) => void;
}

export function StoryBoard({
  stories,
  projectId,
  onOpenStory,
  onOpenFullCreate,
}: StoryBoardProps) {
  const updateStory = useStoryStore((s) => s.updateStory);
  const [activeStory, setActiveStory] = useState<Story | null>(null);
  const [localStories, setLocalStories] = useState<Story[]>(stories);

  useEffect(() => {
    setLocalStories(stories);
  }, [stories]);

  const sort = useStoryViewStore((s) => s.sort);

  const storiesByColumn = useMemo(() => {
    const result = storyStatusOrder.reduce((acc, status) => {
      acc[status] = [];
      return acc;
    }, {} as Record<StoryStatus, Story[]>);

    const priorityWeight: Record<Story["priority"], number> = {
      p0: 0,
      p1: 1,
      p2: 2,
      p3: 3,
    };

    localStories.forEach((story) => {
      result[story.status].push(story);
    });

    storyStatusOrder.forEach((status) => {
      const list = result[status];
      list.sort((a, b) => {
        if (sort === "title") return a.title.localeCompare(b.title);
        if (sort === "updated") {
          return new Date(b.updated_at).getTime() - new Date(a.updated_at).getTime();
        }
        const byPriority = priorityWeight[a.priority] - priorityWeight[b.priority];
        if (byPriority !== 0) return byPriority;
        return new Date(b.updated_at).getTime() - new Date(a.updated_at).getTime();
      });
    });

    return result;
  }, [localStories, sort]);

  const sensors = useSensors(
    useSensor(PointerSensor, {
      activationConstraint: {
        distance: 5,
      },
    }),
    useSensor(KeyboardSensor, {
      coordinateGetter: sortableKeyboardCoordinates,
    }),
  );

  const handleDragStart = (event: DragStartEvent) => {
    const story = stories.find((s) => s.id === event.active.id);
    if (story) {
      setActiveStory(story);
    }
  };

  const handleDragEnd = async (event: DragEndEvent) => {
    const { active, over } = event;
    setActiveStory(null);

    if (!over) return;

    const storyId = active.id as string;
    const overId = over.id as string;

    const overStory = localStories.find((s) => s.id === overId);
    const targetStatus = storyStatusOrder.includes(overId as StoryStatus)
      ? (overId as StoryStatus)
      : overStory?.status;
    if (!targetStatus) return;

    const story = localStories.find((s) => s.id === storyId);

    if (story && story.status !== targetStatus) {
      setLocalStories((prev) =>
        prev.map((s) => (s.id === storyId ? { ...s, status: targetStatus } : s)),
      );

      const updated = await updateStory(storyId, { status: targetStatus });
      if (!updated) {
        setLocalStories(stories);
      }
    }
  };

  return (
    <DndContext
      sensors={sensors}
      collisionDetection={pointerWithin}
      onDragStart={handleDragStart}
      onDragEnd={handleDragEnd}
    >
      <div className="flex h-full min-h-0 gap-4 overflow-x-auto p-1">
        {boardColumns.map((column) => (
          <StoryColumn
            key={column.status}
            column={column}
            stories={storiesByColumn[column.status]}
            projectId={projectId}
            onOpenStory={onOpenStory}
            onOpenFullCreate={onOpenFullCreate}
          />
        ))}
      </div>

      <DragOverlay dropAnimation={null}>
        {activeStory ? (
          <div className="w-[280px] rotate-2 scale-105 cursor-grabbing opacity-90 shadow-lg shadow-foreground/10">
            <StoryCard
              story={activeStory}
              onClick={() => {}}
              isDragging
              inert
            />
          </div>
        ) : null}
      </DragOverlay>
    </DndContext>
  );
}

interface StoryColumnProps {
  column: BoardColumn;
  stories: Story[];
  projectId: string;
  onOpenStory: (story: Story) => void;
  onOpenFullCreate?: (status: StoryStatus) => void;
}

function StoryColumn({
  column,
  stories,
  projectId,
  onOpenStory,
  onOpenFullCreate,
}: StoryColumnProps) {
  const { setNodeRef, isOver } = useDroppable({
    id: column.status,
  });
  const quickAddColumn = useStoryViewStore((s) => s.quickAddColumn);
  const openQuickAddColumn = useStoryViewStore((s) => s.openQuickAddColumn);
  const isQuickAddOpen = quickAddColumn === column.status;

  return (
    <div
      ref={setNodeRef}
      className={`flex w-[280px] shrink-0 flex-col rounded-[12px] bg-secondary/20 p-2 transition-colors ${
        isOver ? "bg-secondary/30 ring-2 ring-primary/50 ring-inset" : ""
      }`}
    >
      <div className="mb-2 flex items-center justify-between px-1.5 py-0.5">
        <StoryStatusToken status={column.status} count={stories.length} />
        <div className="flex items-center gap-1">
          {onOpenFullCreate && (
            <button
              type="button"
              onClick={() => onOpenFullCreate(column.status)}
              className="inline-flex h-6 items-center rounded-[6px] px-1.5 text-[10px] font-medium text-muted-foreground transition-colors hover:bg-background hover:text-foreground"
              title="完整表单创建"
            >
              详细
            </button>
          )}
          <button
            type="button"
            onClick={() => openQuickAddColumn(isQuickAddOpen ? null : column.status)}
            className="inline-flex h-6 w-6 items-center justify-center rounded-[6px] text-muted-foreground transition-colors hover:bg-background hover:text-foreground"
            aria-label={`Quick add ${column.status} story`}
          >
            +
          </button>
        </div>
      </div>

      <div
        className={`min-h-[200px] flex-1 overflow-y-auto rounded-[8px] p-1 transition-colors ${
          isOver ? "bg-background/50" : ""
        }`}
      >
        {isQuickAddOpen && (
          <StoryQuickAdd
            status={column.status}
            projectId={projectId}
            onClose={() => openQuickAddColumn(null)}
          />
        )}
        <SortableContext
          items={stories.map((s) => s.id)}
          strategy={verticalListSortingStrategy}
        >
          <div className="flex flex-col gap-2">
            {stories.map((story) => (
              <SortableStoryCard
                key={story.id}
                story={story}
                onClick={() => onOpenStory(story)}
                showHoverDescription
                selectable
              />
            ))}
            {stories.length === 0 && !isQuickAddOpen && (
              <button
                type="button"
                onClick={() => openQuickAddColumn(column.status)}
                className="rounded-[8px] border border-dashed border-border px-3 py-6 text-center text-[11px] text-muted-foreground transition-colors hover:border-primary/30 hover:bg-background hover:text-foreground"
              >
                + Create in this column
              </button>
            )}
          </div>
        </SortableContext>
      </div>
    </div>
  );
}
