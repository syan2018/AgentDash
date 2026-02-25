import { useEffect, useMemo, useState } from "react";
import type { Story, Task } from "../types";
import { useCoordinatorStore } from "../stores/coordinatorStore";
import { useStoryStore } from "../stores/storyStore";
import { StoryListView } from "../features/story/story-list-view";
import { StoryDrawer } from "../features/story/story-drawer";
import { TaskDrawer } from "../features/task/task-drawer";

export function DashboardPage() {
  const { currentBackendId } = useCoordinatorStore();
  const { stories, tasksByStoryId, isLoading, error, fetchStories, fetchTasks } = useStoryStore();

  const [openedStory, setOpenedStory] = useState<Story | null>(null);
  const [openedTask, setOpenedTask] = useState<Task | null>(null);

  useEffect(() => {
    if (!currentBackendId) return;
    void fetchStories(currentBackendId);
  }, [currentBackendId, fetchStories]);

  const taskCountByStoryId = useMemo(() => {
    const result: Record<string, number> = {};
    stories.forEach((story) => {
      result[story.id] = tasksByStoryId[story.id]?.length ?? story.taskIds.length;
    });
    return result;
  }, [stories, tasksByStoryId]);

  const openedStoryTasks = openedStory ? tasksByStoryId[openedStory.id] ?? [] : [];

  const handleOpenStory = async (story: Story) => {
    setOpenedTask(null);
    setOpenedStory(story);
    if (!tasksByStoryId[story.id]) {
      await fetchTasks(story.id);
    }
  };

  const handleOpenTask = (task: Task) => {
    setOpenedTask(task);
  };

  if (!currentBackendId) {
    return (
      <div className="flex h-full items-center justify-center">
        <div className="text-center">
          <h2 className="text-xl font-semibold text-foreground">请选择后端连接</h2>
          <p className="mt-2 text-sm text-muted-foreground">选择后端后可查看 Story 与 Task 执行状态</p>
        </div>
      </div>
    );
  }

  if (isLoading) {
    return (
      <div className="flex h-full items-center justify-center">
        <div className="h-6 w-6 animate-spin rounded-full border-2 border-primary border-t-transparent" />
      </div>
    );
  }

  return (
    <div className="relative h-full">
      {error && (
        <div className="border-b border-destructive/40 bg-destructive/10 px-6 py-2 text-sm text-destructive">
          数据加载异常：{error}
        </div>
      )}
      <StoryListView
        stories={stories}
        taskCountByStoryId={taskCountByStoryId}
        onOpenStory={(story) => void handleOpenStory(story)}
      />
      <StoryDrawer
        story={openedStory}
        tasks={openedStoryTasks}
        onClose={() => setOpenedStory(null)}
        onOpenTask={handleOpenTask}
      />
      <TaskDrawer task={openedTask} onClose={() => setOpenedTask(null)} />
    </div>
  );
}
