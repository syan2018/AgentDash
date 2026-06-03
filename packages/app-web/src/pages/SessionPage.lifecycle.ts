import type { AgentFrameRuntimeView, LifecycleAgentView } from "../types";

export interface SessionLifecycleDetailTarget {
  runId: string;
  agentId: string | null;
  frameId: string | null;
}

export function resolveSessionLifecycleRunId({
  currentSessionId,
  activeWorkflowRunId,
  activeFrame,
  agents,
}: {
  currentSessionId: string | null;
  activeWorkflowRunId: string | null;
  activeFrame: AgentFrameRuntimeView | null;
  agents: ReadonlyMap<string, LifecycleAgentView>;
}): string | null {
  if (!currentSessionId) return null;

  if (activeWorkflowRunId) return activeWorkflowRunId;
  if (!activeFrame) return null;

  const frameOwnsSession = activeFrame.runtime_session_refs.some(
    (ref) => ref.runtime_session_id === currentSessionId,
  );
  if (!frameOwnsSession) return null;

  const agent = agents.get(activeFrame.frame_ref.agent_id) ?? null;
  return agent?.agent_ref.run_id ?? null;
}

export function resolveSessionLifecycleDetailTarget({
  currentSessionId,
  activeWorkflowRunId,
  activeFrame,
  agents,
}: {
  currentSessionId: string | null;
  activeWorkflowRunId: string | null;
  activeFrame: AgentFrameRuntimeView | null;
  agents: ReadonlyMap<string, LifecycleAgentView>;
}): SessionLifecycleDetailTarget | null {
  const runId = resolveSessionLifecycleRunId({
    currentSessionId,
    activeWorkflowRunId,
    activeFrame,
    agents,
  });
  if (!runId) return null;

  const agentId = activeFrame?.frame_ref.agent_id ?? null;
  return {
    runId,
    agentId,
    frameId: activeFrame?.frame_ref.frame_id ?? null,
  };
}
