import { useEffect } from "react";
import { useNavigate } from "react-router-dom";
import type { Story } from "../../types";
import { useProjectStore } from "../../stores/projectStore";
import { useStoryStore } from "../../stores/storyStore";
import { useStoryViewStore } from "../../stores/storyViewStore";
import { StoryListView } from "./story-list-view";
import { StoryBulkToolbar } from "./story-bulk-toolbar";
import { StoryQuickJump } from "./story-quick-jump";
import { useStoryHotkeys } from "./story-keyboard";

const EMPTY_STORIES: Story[] = [];

export function StoryTabView() {
  const navigate = useNavigate();
  const currentProjectId = useProjectStore((s) => s.currentProjectId);
  const isLoading = useStoryStore((s) => s.isLoading);
  const fetchStoriesByProject = useStoryStore((s) => s.fetchStoriesByProject);
  const stories = useStoryStore((s) => {
    if (!currentProjectId) return EMPTY_STORIES;
    return s.storiesByProjectId[currentProjectId] ?? EMPTY_STORIES;
  });
  const clearSelection = useStoryViewStore((s) => s.clearSelection);

  useStoryHotkeys({ scope: "tab" });

  useEffect(() => {
    if (!currentProjectId) return;
    void fetchStoriesByProject(currentProjectId);
  }, [currentProjectId, fetchStoriesByProject]);

  useEffect(() => {
    return () => {
      clearSelection();
    };
  }, [clearSelection]);

  useEffect(() => {
    clearSelection();
  }, [clearSelection, currentProjectId]);

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
        {/* eslint-disable-next-line no-restricted-syntax -- 圆形 loading spinner，rounded-full 是必要形态 */}
        <div className="h-6 w-6 animate-spin rounded-full border-2 border-primary border-t-transparent" />
      </div>
    );
  }

  return (
    <>
      <StoryListView
        stories={stories}
        onOpenStory={handleOpenStory}
        projectId={currentProjectId}
      />
      <StoryBulkToolbar />
      <StoryQuickJump projectId={currentProjectId} />
    </>
  );
}
