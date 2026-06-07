/**
 * AgentTabView — ProjectAgent Draft 会话入口。
 */

import { useCallback, useEffect, useRef, useState } from "react";
import { useNavigate } from "react-router-dom";
import type { ProjectAgentSummary } from "../../types";
import { useLifecycleStore } from "../../stores/lifecycleStore";
import { useProjectStore } from "../../stores/projectStore";
import { useWorkflowStore } from "../../stores/workflowStore";
import { ActiveSessionList } from "./active-session-list";
import { projectAgentDraftSessionPath } from "./project-agent-paths";
import { ProjectAgentView } from "../project/project-agent-view";

export function AgentTabView() {
  const navigate = useNavigate();
  const {
    currentProjectId,
    projects,
    agentsByProjectId,
    fetchProjectAgents,
    isLoading: projectLoading,
    error: projectError,
  } = useProjectStore();
  const lifecycleLoading = useLifecycleStore((s) => s.isLoading);

  const [selectedAgent, setSelectedAgent] = useState<{
    projectId: string;
    agentId: string;
  } | null>(null);

  const fetchLifecycles = useWorkflowStore((s) => s.fetchLifecycles);
  const fetchDefinitions = useWorkflowStore((s) => s.fetchDefinitions);

  const currentProject = projects.find((p) => p.id === currentProjectId);
  const agents: ProjectAgentSummary[] = currentProjectId
    ? (agentsByProjectId[currentProjectId] ?? [])
    : [];

  const prevProjectIdRef = useRef<string | null>(null);
  useEffect(() => {
    if (!currentProjectId) return;
    if (prevProjectIdRef.current === currentProjectId) return;
    prevProjectIdRef.current = currentProjectId;

    void fetchProjectAgents(currentProjectId);
    void fetchLifecycles({ projectId: currentProjectId });
    void fetchDefinitions({ projectId: currentProjectId });
  }, [currentProjectId, fetchProjectAgents, fetchLifecycles, fetchDefinitions]);

  const selectedAgentId =
    selectedAgent?.projectId === currentProjectId ? selectedAgent.agentId : null;

  const handleLaunchAgent = useCallback(
    (agent: ProjectAgentSummary) => {
      if (!currentProjectId) return;
      navigate(projectAgentDraftSessionPath(currentProjectId, agent.key), {
        state: {
          trace_agent: {
            display_name: agent.display_name,
            executor_hint: agent.executor.executor,
          },
        },
      });
    },
    [currentProjectId, navigate],
  );

  const handleOpenSession = useCallback(
    (runtimeSessionId: string, agentId?: string) => {
      if (!currentProjectId) return;
      if (agentId) {
        setSelectedAgent({ projectId: currentProjectId, agentId });
      }
      navigate(`/session/${runtimeSessionId}`);
    },
    [currentProjectId, navigate],
  );

  if (!currentProjectId || !currentProject) {
    return (
      <div className="flex h-full items-center justify-center">
        <div className="text-center">
          <h2 className="text-xl font-semibold text-foreground">请选择或创建项目</h2>
          <p className="mt-2 text-sm text-muted-foreground">在左侧面板选择一个项目开始使用</p>
        </div>
      </div>
    );
  }

  return (
    <div className="flex h-full overflow-hidden">
      <aside className="flex h-full w-[360px] shrink-0 flex-col overflow-y-auto border-r border-border">
        <ProjectAgentView
          project={currentProject}
          agents={agents}
          isLoading={projectLoading}
          error={projectError}
          onOpenAgent={handleLaunchAgent}
        />
      </aside>

      <div className="flex flex-1 flex-col overflow-hidden">
        <ActiveSessionList
          projectId={currentProjectId}
          isLoading={lifecycleLoading}
          selectedAgentId={selectedAgentId}
          onOpenSession={handleOpenSession}
        />
      </div>
    </div>
  );
}
