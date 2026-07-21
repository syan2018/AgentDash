import type { AgentLiveEvent } from "../../../generated/agent-service-api";
import type { CanonicalConversationRecord } from "../../../generated/backbone-protocol";
import type { ManagedRuntimeSnapshot } from "../../../generated/agent-runtime-validators";

/** Derives execution liveness exclusively from canonical turn boundaries. */
export function hasActiveCanonicalTurn(
  records: readonly CanonicalConversationRecord[],
): boolean {
  const activeTurns = new Set<string>();
  for (const record of records) {
    const event = record.presentation.envelope.event;
    if (event.type === "turn_started") {
      activeTurns.add(event.payload.turn.id);
    } else if (event.type === "turn_completed") {
      activeTurns.delete(event.payload.turn.id);
    }
  }
  return activeTurns.size > 0;
}

/** Adds one process-local canonical conversation record to the disposable feed baseline. */
export function applyAgentLiveEvent(
  snapshot: ManagedRuntimeSnapshot,
  event: AgentLiveEvent,
): ManagedRuntimeSnapshot {
  const existingIndex = snapshot.conversation_history.findIndex(
    (record) => record.presentation_id === event.record.presentation_id,
  );
  const conversationHistory = [...snapshot.conversation_history];
  if (existingIndex >= 0) {
    conversationHistory[existingIndex] = event.record;
  } else {
    conversationHistory.push(event.record);
  }
  return {
    ...snapshot,
    conversation_history: conversationHistory,
  };
}
