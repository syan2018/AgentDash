import type {
  AgentRuntimeUpdate,
  AgentRuntimeView,
} from "../../../generated/agent-runtime-validators";

/** Applies one Runtime-owned control/presentation observation to the rebuildable view. */
export function applyAgentRuntimeUpdate(
  view: AgentRuntimeView,
  update: AgentRuntimeUpdate,
): AgentRuntimeView {
  const conversation = [...update.observation.conversation];
  for (const record of update.presentations) {
    const existingIndex = conversation.findIndex(
      (candidate) => candidate.presentation_id === record.presentation_id,
    );
    if (existingIndex >= 0) {
      conversation[existingIndex] = record;
    } else {
      conversation.push(record);
    }
  }
  return {
    ...view,
    observation: {
      ...update.observation,
      conversation,
    },
  };
}
