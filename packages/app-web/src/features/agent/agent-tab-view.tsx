/**
 * AgentTabView — ProjectAgent launch + lifecycle 执行入口。
 */

import { useCallback, useEffect, useRef, useState } from "react";
import { useNavigate } from "react-router-dom";
import type { ProjectAgentSummary } from "../../types";
import { useLifecycleStore } from "../../stores/lifecycleStore";
import { useProjectStore } from "../../stores/projectStore";
import { useWorkflowStore } from "../../stores/workflowStore";
import { ActiveLifecycleList } from "./active-session-list";
import { ProjectAgentView } from "../project/project-agent-view";

export function AgentTabView() {
  const navigate = useNavigate();
  const {
    currentProjectId,
    projects,
    agentsByProjectId,
    fetchProjectAgents,
    launchProjectAgent,
    isLoading: projectLoading,
    error: projectError,
  } = useProjectStore();
  const fetchAndIngestRun = useLifecycleStore((s) => s.fetchAndIngestRun);
  const fetchFrame = useLifecycleStore((s) => s.fetchFrame);
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
    async (agent: ProjectAgentSummary) => {
      if (!currentProjectId) return;
      const result = await launchProjectAgent(currentProjectId, agent.key);
      if (!result) return;

      await Promise.allSettled([
        fetchAndIngestRun(result.run_ref.run_id),
        fetchFrame(result.frame_ref.frame_id),
      ]);
      setSelectedAgent({
        projectId: currentProjectId,
        agentId: result.agent_ref.agent_id,
      });
      navigate(`/agent/${result.agent_ref.agent_id}`, {
        state: {
          run_id: result.run_ref.run_id,
          frame_id: result.frame_ref.frame_id,
          runtime_session_id: result.runtime_session_ref?.runtime_session_id ?? null,
        },
      });
    },
    [currentProjectId, fetchAndIngestRun, fetchFrame, launchProjectAgent, navigate],
  );

  const handleSelectAgent = useCallback(
    (runId: string, agentId: string) => {
      if (!currentProjectId) return;
      setSelectedAgent({ projectId: currentProjectId, agentId });
      navigate(`/agent/${agentId}`, { state: { run_id: runId } });
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
        <ActiveLifecycleList
          projectId={currentProjectId}
          isLoading={lifecycleLoading}
          selectedAgentId={selectedAgentId}
          onSelectAgent={handleSelectAgent}
        />
      </div>
    </div>
  );
}
