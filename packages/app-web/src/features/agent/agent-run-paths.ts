export function agentRunDraftPath(projectId: string, projectAgentId: string): string {
  const params = new URLSearchParams();
  params.set("project_id", projectId);
  params.set("project_agent_id", projectAgentId);
  return `/agent-runs/new?${params.toString()}`;
}

export function agentRunWorkspacePath(runId: string, agentId: string): string {
  return `/agent-runs/${encodeURIComponent(runId)}/${encodeURIComponent(agentId)}`;
}

