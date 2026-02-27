import { useEffect, useMemo, useState } from "react";
import type { Story, Task } from "../types";
import { useProjectStore } from "../stores/projectStore";
import { useStoryStore } from "../stores/storyStore";
import { useWorkspaceStore } from "../stores/workspaceStore";
import { StoryListView } from "../features/story/story-list-view";
import { StoryDrawer } from "../features/story/story-drawer";
import { TaskDrawer } from "../features/task/task-drawer";

export function DashboardPage() {
  const { currentProjectId, projects } = useProjectStore();
  const { stories, tasksByStoryId, isLoading, error, fetchStoriesByProject, fetchTasks } = useStoryStore();
  const { workspacesByProjectId } = useWorkspaceStore();

  const [openedStory, setOpenedStory] = useState<Story | null>(null);
  const [openedTask, setOpenedTask] = useState<Task | null>(null);

  const currentProject = projects.find((p) => p.id === currentProjectId);

  useEffect(() => {
    if (!currentProjectId) return;
    void fetchStoriesByProject(currentProjectId);
  }, [currentProjectId, fetchStoriesByProject]);

  const taskCountByStoryId = useMemo(() => {
    const result: Record<string, number> = {};
    stories.forEach((story) => {
      result[story.id] = tasksByStoryId[story.id]?.length ?? 0;
    });
    return result;
  }, [stories, tasksByStoryId]);

  const openedStoryTasks = openedStory ? tasksByStoryId[openedStory.id] ?? [] : [];
  const currentWorkspaces = currentProjectId ? workspacesByProjectId[currentProjectId] ?? [] : [];

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

  if (!currentProjectId) {
    return (
      <div className="flex h-full items-center justify-center">
        <div className="text-center">
          <h2 className="text-xl font-semibold text-foreground">请选择或创建项目</h2>
          <p className="mt-2 text-sm text-muted-foreground">在左侧面板选择一个项目，或创建新项目开始使用</p>
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
        projectId={currentProjectId}
        backendId={currentProject?.backend_id ?? ""}
      />
      <StoryDrawer
        story={openedStory}
        tasks={openedStoryTasks}
        workspaces={currentWorkspaces}
        projectConfig={currentProject?.config}
        onClose={() => setOpenedStory(null)}
        onOpenTask={handleOpenTask}
      />
      <TaskDrawer task={openedTask} onClose={() => setOpenedTask(null)} />
    </div>
  );
}
