export function projectAgentDraftSessionPath(projectId: string, agentKey: string): string {
  const params = new URLSearchParams({
    project_id: projectId,
    project_agent_id: agentKey,
  });
  return `/session/new?${params.toString()}`;
}
