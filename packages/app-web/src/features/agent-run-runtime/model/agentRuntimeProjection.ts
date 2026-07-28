import type {
  AgentRuntimeUpdate,
  AgentRuntimeView,
} from "../../../generated/agent-runtime-validators";

/** Applies one Runtime-owned control/presentation observation to the rebuildable view. */
export function applyAgentRuntimeUpdate(
  view: AgentRuntimeView,
  update: AgentRuntimeUpdate,
): AgentRuntimeView {
  const conversation = [...view.conversation];
  let threadName = view.thread_name;
  for (const record of update.presentations) {
    const existingIndex = conversation.findIndex(
      (candidate) => candidate.presentation_id === record.presentation_id,
    );
    if (existingIndex >= 0) {
      conversation[existingIndex] = record;
    } else {
      conversation.push(record);
    }
    const event = record.presentation.envelope.event;
    if (event.type === "thread_name_updated") {
      threadName = event.payload.threadName ?? null;
    }
  }
  return {
    ...view,
    view_revision: update.view_revision,
    execution: update.execution,
    interactions: update.interactions,
    command_availability: update.command_availability,
    thread_name: threadName,
    conversation,
  };
}
