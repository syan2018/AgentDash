import { useEffect, useMemo, useState } from "react";
import { useNavigate } from "react-router-dom";
import type { ProjectAgentSummary, SessionNavigationState, Story } from "../types";
import { useProjectStore } from "../stores/projectStore";
import { useStoryStore } from "../stores/storyStore";
import { StoryListView } from "../features/story/story-list-view";
import { ProjectAgentView } from "../features/project/project-agent-view";

export function DashboardPage() {
  const navigate = useNavigate();
  const [activeView, setActiveView] = useState<"stories" | "agents">("stories");
  const {
    currentProjectId,
    projects,
    agentsByProjectId,
    fetchProjectAgents,
    openProjectAgentSession,
    error: projectError,
  } = useProjectStore();
  const { stories, isLoading, error: storyError, fetchStoriesByProject } = useStoryStore();

  const currentProject = projects.find((p) => p.id === currentProjectId);
  const currentAgents = currentProjectId ? (agentsByProjectId[currentProjectId] ?? []) : [];

  useEffect(() => {
    if (!currentProjectId) return;
    if (activeView === "stories") {
      void fetchStoriesByProject(currentProjectId);
      return;
    }
    void fetchProjectAgents(currentProjectId);
  }, [activeView, currentProjectId, fetchProjectAgents, fetchStoriesByProject]);

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

  const handleOpenAgent = async (agent: ProjectAgentSummary) => {
    if (!currentProjectId) return;
    const result = await openProjectAgentSession(currentProjectId, agent.key);
    if (!result) return;

    const navigationState: SessionNavigationState = {
      return_to: {
        owner_type: "project",
        project_id: currentProjectId,
      },
      project_agent: {
        agent_key: result.agent.key,
        display_name: result.agent.display_name,
        executor_hint: result.agent.executor.executor,
      },
    };
    navigate(`/session/${result.session_id}`, { state: navigationState });
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

  if (activeView === "stories" && isLoading && stories.length === 0) {
    return (
      <div className="flex h-full items-center justify-center">
        <div className="h-6 w-6 animate-spin rounded-full border-2 border-primary border-t-transparent" />
      </div>
    );
  }

  return (
    <div className="relative h-full">
      <div className="border-b border-border bg-background px-6 py-3">
        <div className="inline-flex rounded-[12px] border border-border bg-secondary/30 p-1">
          <button
            type="button"
            onClick={() => setActiveView("stories")}
            className={`rounded-[10px] px-3 py-1.5 text-sm transition-colors ${
              activeView === "stories"
                ? "bg-background text-foreground shadow-sm"
                : "text-muted-foreground hover:text-foreground"
            }`}
          >
            Story 视图
          </button>
          <button
            type="button"
            onClick={() => setActiveView("agents")}
            className={`rounded-[10px] px-3 py-1.5 text-sm transition-colors ${
              activeView === "agents"
                ? "bg-background text-foreground shadow-sm"
                : "text-muted-foreground hover:text-foreground"
            }`}
          >
            Agent 视图
          </button>
        </div>
      </div>
      {activeView === "stories" && isLoading && stories.length > 0 && (
        <div className="border-b border-border bg-muted/40 px-6 py-1 text-xs text-muted-foreground">
          正在刷新看板数据...
        </div>
      )}
      {activeView === "stories" && storyError && (
        <div className="border-b border-destructive/40 bg-destructive/10 px-6 py-2 text-sm text-destructive">
          数据加载异常：{storyError}
        </div>
      )}
      {activeView === "stories" ? (
        <StoryListView
          stories={stories}
          taskCountByStoryId={taskCountByStoryId}
          onOpenStory={handleOpenStory}
          projectId={currentProjectId}
          backendId={currentProject?.backend_id ?? ""}
        />
      ) : (
        <ProjectAgentView
          projectName={currentProject?.name ?? "当前项目"}
          agents={currentAgents}
          error={projectError}
          onOpenAgent={handleOpenAgent}
        />
      )}
    </div>
  );
}
