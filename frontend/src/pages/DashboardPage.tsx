import { useEffect, useMemo } from "react";
import { useNavigate } from "react-router-dom";
import type { Story } from "../types";
import { useProjectStore } from "../stores/projectStore";
import { useStoryStore } from "../stores/storyStore";
import { StoryListView } from "../features/story/story-list-view";

export function DashboardPage() {
  const navigate = useNavigate();
  const { currentProjectId, projects } = useProjectStore();
  const { stories, tasksByStoryId, isLoading, error, fetchStoriesByProject } = useStoryStore();

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

  const handleOpenStory = (story: Story) => {
    navigate(`/story/${story.id}`);
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
        onOpenStory={handleOpenStory}
        projectId={currentProjectId}
        backendId={currentProject?.backend_id ?? ""}
      />
    </div>
  );
}
