/**
 * StoryTabView — Story 看板 Tab 入口
 *
 * 从 store 拉取数据后代理传入 StoryListView，
 * 保持 StoryListView 原有 Props 接口不变。
 */

import { useEffect, useMemo } from "react";
import { useNavigate } from "react-router-dom";
import type { Story } from "../../types";
import { useProjectStore } from "../../stores/projectStore";
import { useStoryStore } from "../../stores/storyStore";
import { StoryListView } from "./story-list-view";

export function StoryTabView() {
  const navigate = useNavigate();
  const currentProjectId = useProjectStore((s) => s.currentProjectId);
  const { stories, isLoading, fetchStoriesByProject } = useStoryStore();

  // 切换项目或首次渲染时加载 Story 列表
  useEffect(() => {
    if (!currentProjectId) return;
    void fetchStoriesByProject(currentProjectId);
  }, [currentProjectId, fetchStoriesByProject]);

  const taskCountByStoryId = useMemo(() => {
    const result: Record<string, number> = {};
    stories.forEach((story) => {
      result[story.id] = story.task_count ?? 0;
    });
    return result;
  }, [stories]);

  const handleOpenStory = (story: Story) => {
    navigate(`/story/${story.id}`);
  };

  if (!currentProjectId) {
    return (
      <div className="flex h-full items-center justify-center">
        <div className="text-center">
          <h2 className="text-xl font-semibold text-foreground">请选择或创建项目</h2>
          <p className="mt-2 text-sm text-muted-foreground">在左侧面板选择一个项目开始使用</p>
        </div>
      </div>
    );
  }

  if (isLoading && stories.length === 0) {
    return (
      <div className="flex h-full items-center justify-center">
        <div className="h-6 w-6 animate-spin rounded-full border-2 border-primary border-t-transparent" />
      </div>
    );
  }

  return (
    <StoryListView
      stories={stories}
      taskCountByStoryId={taskCountByStoryId}
      onOpenStory={handleOpenStory}
      projectId={currentProjectId}
    />
  );
}
