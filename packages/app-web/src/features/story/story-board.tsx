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

const storyStatusOrder: StoryStatus[] = [
  "draft",
  "ready",
  "running",
  "review",
  "completed",
  "failed",
  "cancelled",
];

interface BoardColumn {
  status: StoryStatus;
}

const boardColumns: BoardColumn[] = [
  { status: "draft" },
  { status: "ready" },
  { status: "running" },
  { status: "review" },
  { status: "completed" },
  { status: "failed" },
  { status: "cancelled" },
];

interface StoryBoardProps {
  stories: Story[];
  taskCountByStoryId: Record<string, number>;
  onCreateStory?: (status: StoryStatus) => void;
  onOpenStory: (story: Story) => void;
}

export function StoryBoard({ stories, taskCountByStoryId, onCreateStory, onOpenStory }: StoryBoardProps) {
  const { updateStory } = useStoryStore();
  const [activeStory, setActiveStory] = useState<Story | null>(null);
  const [localStories, setLocalStories] = useState<Story[]>(stories);

  useEffect(() => {
    setLocalStories(stories);
  }, [stories]);

  const storiesByColumn = useMemo(() => {
    const result = storyStatusOrder.reduce((acc, status) => {
      acc[status] = [];
      return acc;
    }, {} as Record<StoryStatus, Story[]>);

    const priorityWeight: Record<Story['priority'], number> = {
      p0: 0,
      p1: 1,
      p2: 2,
      p3: 3,
    };

    const sortByPriority = (a: Story, b: Story) => {
      return priorityWeight[a.priority] - priorityWeight[b.priority];
    };

    localStories.forEach((story) => {
      result[story.status].push(story);
    });

    storyStatusOrder.forEach((status) => {
      result[status].sort(sortByPriority);
    });

    return result;
  }, [localStories]);

  const sensors = useSensors(
    useSensor(PointerSensor, {
      activationConstraint: {
        distance: 5,
      },
    }),
    useSensor(KeyboardSensor, {
      coordinateGetter: sortableKeyboardCoordinates,
    })
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
      ? overId as StoryStatus
      : overStory?.status;
    if (!targetStatus) return;

    const story = localStories.find((s) => s.id === storyId);

    if (story && story.status !== targetStatus) {
      setLocalStories((prev) => prev.map((s) => (s.id === storyId ? { ...s, status: targetStatus } : s)));

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
            taskCountByStoryId={taskCountByStoryId}
            onCreateStory={onCreateStory}
            onOpenStory={onOpenStory}
          />
        ))}
      </div>

      <DragOverlay dropAnimation={null}>
        {activeStory ? (
          <div className="w-[280px] rotate-2 scale-105 cursor-grabbing opacity-90 shadow-lg shadow-foreground/10">
            <StoryCard
              story={activeStory}
              taskCount={taskCountByStoryId[activeStory.id] ?? 0}
              onClick={() => {}}
              isDragging
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
  taskCountByStoryId: Record<string, number>;
  onCreateStory?: (status: StoryStatus) => void;
  onOpenStory: (story: Story) => void;
}

function StoryColumn({ column, stories, taskCountByStoryId, onCreateStory, onOpenStory }: StoryColumnProps) {
  const { setNodeRef, isOver } = useDroppable({
    id: column.status,
  });

  return (
    <div
      ref={setNodeRef}
      className={`flex w-[280px] shrink-0 flex-col rounded-[12px] bg-secondary/20 p-2 transition-colors ${
        isOver ? "bg-secondary/30 ring-2 ring-primary/50 ring-inset" : ""
      }`}
    >
      <div className="mb-2 flex items-center justify-between px-1.5 py-0.5">
        <StoryStatusToken status={column.status} count={stories.length} />
        {onCreateStory && (
          <button
            type="button"
            onClick={() => onCreateStory(column.status)}
            className="inline-flex h-6 w-6 items-center justify-center rounded-[6px] text-muted-foreground transition-colors hover:bg-background hover:text-foreground"
            aria-label={`Create ${column.status} story`}
          >
            +
          </button>
        )}
      </div>

      <div
        className={`min-h-[200px] flex-1 overflow-y-auto rounded-[8px] p-1 transition-colors ${
          isOver ? "bg-background/50" : ""
        }`}
      >
        <SortableContext
          items={stories.map((s) => s.id)}
          strategy={verticalListSortingStrategy}
        >
          <div className="flex flex-col gap-2">
            {stories.map((story) => (
              <SortableStoryCard
                key={story.id}
                story={story}
                taskCount={taskCountByStoryId[story.id] ?? 0}
                onClick={() => onOpenStory(story)}
              />
            ))}
          </div>
        </SortableContext>
      </div>
    </div>
  );
}
