import type {
  AgentRunProductLineageAgentView,
  AgentRunProductLineageView,
} from "../../../generated/workflow-contracts";
import { agentRunListPresentationStatus } from "../../agent/agent-run-delivery-status";
import type { CompanionSubagentKnownAgentRef } from "../../session/model/companionSubagentDispatch";

export function collectAgentRunProductCompanionRefs(
  lineage: AgentRunProductLineageView | null | undefined,
): CompanionSubagentKnownAgentRef[] {
  const refs: CompanionSubagentKnownAgentRef[] = [];
  for (const child of lineage?.children ?? []) {
    appendCompanionRef(refs, child);
  }
  return refs;
}

function appendCompanionRef(
  refs: CompanionSubagentKnownAgentRef[],
  agent: AgentRunProductLineageAgentView,
): void {
  refs.push({
    run_id: agent.run_ref.run_id,
    agent_id: agent.agent_ref.agent_id,
    display_title: agent.title,
    delivery_status: agentRunListPresentationStatus(
      agent.runtime?.thread_status,
      agent.runtime?.active_turn_id,
      agent.lifecycle_status,
    ),
    last_activity_at: agent.last_activity_at,
  });
  for (const child of agent.children) {
    appendCompanionRef(refs, child);
  }
}
