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

// 看板列定义（4列布局）
type BoardColumnKey = "todo" | "inprogress" | "inreview" | "done";

interface BoardColumn {
  key: BoardColumnKey;
  label: string;
  color: string;
  bgColor: string;
  accepts: StoryStatus[];
}

// 看板列样式与 status-badge 保持一致
const boardColumns: BoardColumn[] = [
  {
    key: "todo",
    label: "Todo",
    color: "text-muted-foreground",
    bgColor: "bg-muted",
    accepts: ["draft", "ready"],
  },
  {
    key: "inprogress",
    label: "In Progress",
    color: "text-primary",
    bgColor: "bg-primary/15",
    accepts: ["running"],
  },
  {
    key: "inreview",
    label: "In Review",
    color: "text-warning",
    bgColor: "bg-warning/15",
    accepts: ["review"],
  },
  {
    key: "done",
    label: "Done",
    color: "text-success",
    bgColor: "bg-success/15",
    accepts: ["completed", "failed", "cancelled"],
  },
];

// Story 状态到看板列的映射
const statusToColumnMap: Record<StoryStatus, BoardColumnKey> = {
  draft: "todo",
  ready: "todo",
  running: "inprogress",
  review: "inreview",
  completed: "done",
  failed: "done",
  cancelled: "done",
};

// 看板列到默认状态的映射（用于拖拽时设置新状态）
const columnToDefaultStatus: Record<BoardColumnKey, StoryStatus> = {
  todo: "draft",
  inprogress: "running",
  inreview: "review",
  done: "completed",
};

interface StoryBoardProps {
  stories: Story[];
  taskCountByStoryId: Record<string, number>;
  onOpenStory: (story: Story) => void;
}

export function StoryBoard({ stories, taskCountByStoryId, onOpenStory }: StoryBoardProps) {
  const { updateStory } = useStoryStore();
  const [activeStory, setActiveStory] = useState<Story | null>(null);
  // 乐观更新：本地状态覆盖 props
  const [localStories, setLocalStories] = useState<Story[]>(stories);

  // 同步外部 stories 到本地
  useEffect(() => {
    setLocalStories(stories);
  }, [stories]);

  // 将 Story 按看板列分组（使用本地状态）
  const storiesByColumn = useMemo(() => {
    const result: Record<BoardColumnKey, Story[]> = {
      todo: [],
      inprogress: [],
      inreview: [],
      done: [],
    };
    localStories.forEach((story) => {
      const columnKey = statusToColumnMap[story.status];
      result[columnKey].push(story);
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
    const targetColumn = over.id as BoardColumnKey;

    // 检查是否是有效的列
    const validColumns: BoardColumnKey[] = ["todo", "inprogress", "inreview", "done"];
    if (!validColumns.includes(targetColumn)) return;

    const story = localStories.find((s) => s.id === storyId);
    const currentColumn = story ? statusToColumnMap[story.status] : null;

    // 只有跨列拖拽时才更新状态
    if (story && currentColumn !== targetColumn) {
      const newStatus = columnToDefaultStatus[targetColumn];

      // 乐观更新：立即更新本地状态
      setLocalStories((prev) =>
        prev.map((s) => (s.id === storyId ? { ...s, status: newStatus } : s))
      );

      // 异步调用 API（失败时状态会自动回滚，因为 props 会重新同步）
      await updateStory(storyId, { status: newStatus });
    }
  };

  return (
    <DndContext
      sensors={sensors}
      collisionDetection={pointerWithin}
      onDragStart={handleDragStart}
      onDragEnd={handleDragEnd}
    >
      {/* 4列网格布局，自适应宽度 */}
      <div className="grid h-full grid-cols-4 gap-4 overflow-hidden">
        {boardColumns.map((column) => (
          <StoryColumn
            key={column.key}
            column={column}
            stories={storiesByColumn[column.key]}
            taskCountByStoryId={taskCountByStoryId}
            onOpenStory={onOpenStory}
          />
        ))}
      </div>

      <DragOverlay>
        {activeStory ? (
          <StoryCard
            story={activeStory}
            taskCount={taskCountByStoryId[activeStory.id] ?? 0}
            onClick={() => {}}
            isDragging
          />
        ) : null}
      </DragOverlay>
    </DndContext>
  );
}

interface StoryColumnProps {
  column: BoardColumn;
  stories: Story[];
  taskCountByStoryId: Record<string, number>;
  onOpenStory: (story: Story) => void;
}

function StoryColumn({ column, stories, taskCountByStoryId, onOpenStory }: StoryColumnProps) {
  const { setNodeRef, isOver } = useDroppable({
    id: column.key,
  });

  return (
    <div
      ref={setNodeRef}
      className={`flex h-full flex-col rounded-lg border border-border bg-secondary/30 ${
        isOver ? "ring-2 ring-primary ring-inset" : ""
      }`}
    >
      {/* 列标题 */}
      <div className={`flex items-center justify-between border-b border-border px-3 py-2.5 ${column.bgColor} rounded-t-lg`}>
        <div className="flex items-center gap-2">
          <h3 className={`text-sm font-semibold ${column.color}`}>{column.label}</h3>
          <span className="flex h-5 min-w-[1.25rem] items-center justify-center rounded-full bg-background px-1.5 text-xs font-medium text-muted-foreground">
            {stories.length}
          </span>
        </div>
      </div>

      {/* 卡片列表 */}
      <div className="flex-1 overflow-y-auto p-3">
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

        {stories.length === 0 && (
          <div className="flex h-24 items-center justify-center text-xs text-muted-foreground">
            Drop stories here
          </div>
        )}
      </div>
    </div>
  );
}
