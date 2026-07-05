import { createInitialStreamState, reduceStreamState } from "./sessionStreamReducer";
import type { SessionDisplayEntry, SessionEventEnvelope } from "./types";

export interface AgentRunStreamIdentityTarget {
  runId: string;
  agentId: string;
}

export function agentRunSyntheticSessionId(target: AgentRunStreamIdentityTarget): string {
  return `agentrun:${target.runId}:${target.agentId}`;
}

export function normalizeAgentRunStreamEventIdentity(
  event: SessionEventEnvelope,
  target: AgentRunStreamIdentityTarget,
): SessionEventEnvelope {
  const sessionId = agentRunSyntheticSessionId(target);
  if (event.session_id === sessionId && event.notification.sessionId === sessionId) {
    return event;
  }
  return {
    ...event,
    session_id: sessionId,
    notification: {
      ...event.notification,
      sessionId,
    },
  };
}

export function agentRunSeedEntries(
  events: SessionEventEnvelope[],
  target: AgentRunStreamIdentityTarget,
): SessionDisplayEntry[] {
  if (events.length === 0) return [];
  const normalizedEvents = events.map((event) => normalizeAgentRunStreamEventIdentity(event, target));
  const reduced = reduceStreamState(createInitialStreamState([]), normalizedEvents);
  const count = reduced.entries.length;
  return reduced.entries.map((entry, index) => {
    const seq = index - count;
    return {
      ...entry,
      eventSeq: seq,
      timelineOrder: { kind: "durable", seq },
      isStreaming: false,
      projectedTranscriptStable: true,
    };
  });
}
