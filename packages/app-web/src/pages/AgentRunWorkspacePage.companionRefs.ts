import type { AgentRunListChild, AgentRunWorkspaceListEntry } from "../types";

export interface CompanionSubagentKnownAgentRef {
  run_id: string;
  agent_id: string;
  display_title: string;
  delivery_status: string;
  last_activity_at: string;
}

export function collectCompanionSubagentRefs(
  entries: AgentRunWorkspaceListEntry[],
  currentRunId: string | null,
): CompanionSubagentKnownAgentRef[] {
  const refs: CompanionSubagentKnownAgentRef[] = [];
  for (const entry of entries) {
    if (currentRunId && entry.run_ref.run_id !== currentRunId) continue;
    for (const child of entry.children) {
      appendCompanionSubagentRef(refs, child);
    }
  }
  return refs;
}

function appendCompanionSubagentRef(
  refs: CompanionSubagentKnownAgentRef[],
  child: AgentRunListChild,
): void {
  refs.push({
    run_id: child.run_ref.run_id,
    agent_id: child.agent_ref.agent_id,
    display_title: child.title,
    delivery_status: child.lifecycle_status,
    last_activity_at: child.last_activity_at,
  });
  for (const nested of child.children) {
    appendCompanionSubagentRef(refs, nested);
  }
}
