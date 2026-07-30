import type {
  AgentRuntimeUpdate,
  AgentRuntimeView,
} from "../../../generated/agent-runtime-codecs";

/**
 * Applies only owner-published control state to the authoritative view.
 *
 * Live presentations belong to the process-local lane and are folded by the Session reducer;
 * they are deliberately not accumulated into the authoritative conversation.
 */
export function applyAgentRuntimeUpdate(
  view: AgentRuntimeView,
  update: AgentRuntimeUpdate,
): AgentRuntimeView {
  if (!update.state || update.state.revision < view.observation.revision) {
    return view;
  }
  return {
    ...view,
    observation: {
      ...update.state,
      conversation: view.observation.conversation,
    },
  };
}
