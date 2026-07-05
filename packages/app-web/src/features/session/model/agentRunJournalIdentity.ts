export interface AgentRunJournalIdentityTarget {
  runId: string;
  agentId: string;
}

export function agentRunJournalSessionId(target: AgentRunJournalIdentityTarget): string {
  return `agentrun:${target.runId}:${target.agentId}`;
}
